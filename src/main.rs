use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use std::io::Write;
use std::path::Path;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use mongodb::{Client, options::ClientOptions};
use dotenv::dotenv;

#[derive(Serialize, Deserialize, Clone)]
struct FileInfo {
    id: String,
    delete_id: String,
    original_name: String,
    size: u64,
    upload_date: u64,
}

struct AppState {
    db: mongodb::Database,
}

async fn index() -> impl Responder {
    let content = include_str!("../static/index.html");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(content)
}

async fn download_page() -> impl Responder {
    let content = include_str!("../static/download.html");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(content)
}

async fn delete_page() -> impl Responder {
    let content = include_str!("../static/delete.html");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(content)
}

async fn style() -> impl Responder {
    let content = include_str!("../static/style.css");
    HttpResponse::Ok()
        .content_type("text/css")
        .body(content)
}

async fn upload_icon() -> impl Responder {
    let content = include_str!("../static/upload-icon.svg");
    HttpResponse::Ok()
        .content_type("image/svg+xml")
        .body(content)
}

async fn upload_file(mut payload: Multipart, data: web::Data<AppState>) -> impl Responder {
    let upload_dir = Path::new("uploads");
    if !upload_dir.exists() {
        fs::create_dir_all(upload_dir).unwrap();
    }

    let file_id = Uuid::new_v4().to_string();
    let delete_id = Uuid::new_v4().to_string();
    let file_dir = upload_dir.join(&file_id);
    fs::create_dir_all(&file_dir).unwrap();

    let mut original_name = String::new();
    let mut file_size = 0u64;
    let mut file_path = None;

    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_disposition = field.content_disposition();
        if let Some(filename) = content_disposition.get_filename() {
            original_name = filename.to_string();
            file_path = Some(file_dir.join(&original_name));
        }

        if let Some(path) = &file_path {
            let mut file = fs::File::create(path).unwrap();
            while let Some(chunk) = field.next().await {
                let data = chunk.unwrap();
                file_size += data.len() as u64;
                file.write_all(&data).unwrap();
            }
        }
    }

    let upload_date = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let file_info = FileInfo {
        id: file_id.clone(),
        delete_id: delete_id.clone(),
        original_name,
        size: file_size,
        upload_date,
    };

    // MongoDB'ye kaydet
    let collection = data.db.collection::<FileInfo>("files");
    if let Err(e) = collection.insert_one(&file_info, None).await {
        println!("MongoDB kayıt hatası: {}", e);
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().json(file_info)
}

async fn get_file_info(file_id: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let collection = data.db.collection::<FileInfo>("files");
    
    match collection.find_one(mongodb::bson::doc! { "$or": [
        { "id": &*file_id },
        { "delete_id": &*file_id }
    ]}, None).await {
        Ok(Some(file_info)) => HttpResponse::Ok().json(file_info),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            println!("MongoDB okuma hatası: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn download_file(file_id: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let collection = data.db.collection::<FileInfo>("files");
    
    match collection.find_one(mongodb::bson::doc! { "id": &*file_id }, None).await {
        Ok(Some(file_info)) => {
            let file_path = Path::new("uploads").join(&file_info.id).join(&file_info.original_name);
            if let Ok(file) = fs::read(&file_path) {
                return HttpResponse::Ok()
                    .content_type("application/octet-stream")
                    .append_header(("Content-Disposition", format!("attachment; filename=\"{}\"", file_info.original_name)))
                    .body(file);
            }
            HttpResponse::NotFound().finish()
        }
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            println!("MongoDB okuma hatası: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn delete_file(delete_id: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let collection = data.db.collection::<FileInfo>("files");
    
    match collection.find_one(mongodb::bson::doc! { "delete_id": &*delete_id }, None).await {
        Ok(Some(file_info)) => {
            // Önce dosyayı sil
            let file_dir = Path::new("uploads").join(&file_info.id);
            if let Err(e) = fs::remove_dir_all(&file_dir) {
                println!("Dosya silme hatası: {}", e);
                return HttpResponse::InternalServerError().finish();
            }

            // Sonra MongoDB kaydını sil
            match collection.delete_one(mongodb::bson::doc! { "delete_id": &*delete_id }, None).await {
                Ok(_) => HttpResponse::Ok().finish(),
                Err(e) => {
                    println!("MongoDB silme hatası: {}", e);
                    HttpResponse::InternalServerError().finish()
                }
            }
        }
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            println!("MongoDB okuma hatası: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    
    let mongodb_uri = std::env::var("MONGODB_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".to_string());
    
    let client_options = ClientOptions::parse(&mongodb_uri).await.unwrap();
    let client = Client::with_options(client_options).unwrap();
    let db = client.database("filesite");

    println!("MongoDB bağlantısı başarılı!");
    println!("Sunucu başlatılıyor...");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                db: db.clone(),
            }))
            .route("/", web::get().to(index))
            .route("/download.html", web::get().to(download_page))
            .route("/delete.html", web::get().to(delete_page))
            .route("/static/style.css", web::get().to(style))
            .route("/static/upload-icon.svg", web::get().to(upload_icon))
            .route("/upload", web::post().to(upload_file))
            .route("/file/{id}", web::get().to(get_file_info))
            .route("/download/{id}", web::get().to(download_file))
            .route("/delete/{id}", web::delete().to(delete_file))
    })
    .bind("127.0.0.1:22415")?
    .run()
    .await
}
