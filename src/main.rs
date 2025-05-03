use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use std::io::Write;
use std::path::Path;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct FileInfo {
    id: String,
    original_name: String,
    size: u64,
    upload_date: u64,
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

async fn upload_file(mut payload: Multipart) -> impl Responder {
    let upload_dir = Path::new("uploads");
    if !upload_dir.exists() {
        fs::create_dir_all(upload_dir).unwrap();
    }

    let file_id = Uuid::new_v4().to_string();
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
        id: file_id,
        original_name,
        size: file_size,
        upload_date,
    };

    HttpResponse::Ok().json(file_info)
}

async fn get_file_info(file_id: web::Path<String>) -> impl Responder {
    let upload_dir = Path::new("uploads").join(&*file_id);
    if !upload_dir.exists() {
        return HttpResponse::NotFound().finish();
    }

    if let Ok(entries) = fs::read_dir(&upload_dir) {
        if let Some(entry) = entries.filter_map(Result::ok).next() {
            let metadata = fs::metadata(entry.path()).unwrap();
            let file_info = FileInfo {
                id: file_id.into_inner(),
                original_name: entry.file_name().to_string_lossy().into_owned(),
                size: metadata.len(),
                upload_date: metadata.modified()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            return HttpResponse::Ok().json(file_info);
        }
    }

    HttpResponse::NotFound().finish()
}

async fn download_file(file_id: web::Path<String>) -> impl Responder {
    let upload_dir = Path::new("uploads").join(&*file_id);
    if !upload_dir.exists() {
        return HttpResponse::NotFound().finish();
    }

    if let Ok(entries) = fs::read_dir(&upload_dir) {
        if let Some(entry) = entries.filter_map(Result::ok).next() {
            let file_path = entry.path();
            if let Ok(file) = fs::read(&file_path) {
                let filename = entry.file_name().to_string_lossy().into_owned();
                return HttpResponse::Ok()
                    .content_type("application/octet-stream")
                    .append_header(("Content-Disposition", format!("attachment; filename=\"{}\"", filename)))
                    .body(file);
            }
        }
    }

    HttpResponse::NotFound().finish()
}

async fn delete_file(file_id: web::Path<String>) -> impl Responder {
    let upload_dir = Path::new("uploads").join(&*file_id);
    if !upload_dir.exists() {
        return HttpResponse::NotFound().finish();
    }

    if let Err(e) = fs::remove_dir_all(&upload_dir) {
        println!("Dosya silme hatası: {}", e);
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Sunucu başlatılıyor...");
    HttpServer::new(|| {
        App::new()
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
    .bind("127.0.0.1:22555")?
    .run()
    .await
}
