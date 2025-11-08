use axum::{
    body::Body,
    http::{header, Response, StatusCode},
    response::{IntoResponse, Response as AxumResponse},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

use image::{ImageReader, ImageFormat};

use std::io::{self, Cursor};
use std::path::Path;
use std::fs::{self, File, DirEntry};

use rand::Rng;

use log::{info, LevelFilter};
use simplelog::{CombinedLogger, Config, WriteLogger};

const IMAGE_EXTENSION: [&str; 3] = ["png", "jpg", "jpeg"];

#[tokio::main]
async fn main() {
    CombinedLogger::init(
        vec![
            WriteLogger::new(
                LevelFilter::Info,
                Config::default(),
                File::create("my_rust_binary.log").unwrap()
            ),
        ]
    ).unwrap();

    let app = Router::new()
        .route("/get_random_art", get(get_random_art_handler));
    let addr = SocketAddr::from(( [0, 0, 0, 0], 3000 ));
    info!(" Server started, listening on http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}

enum ImageError {
    IO(std::io::Error),
    Load(image::ImageError),
    Encode(image::ImageError),
}

impl IntoResponse for ImageError {
    fn into_response(self) -> AxumResponse {
        let (status, message) = match self {
            ImageError::IO(e) => {
                let error_msg = format!("Failed during IO image: {}", e);
                info!("{}",error_msg);
                (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
            }
            ImageError::Load(e) => {
                let error_msg = format!("Failed to load image: {}", e);
                info!("{}",error_msg);
                (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
            }
            ImageError::Encode(e) => {
                let error_msg = format!("Failed to encode Image: {}", e);
                info!("{}",error_msg);
                (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
            }
        };
        (status, message.to_string()).into_response()
    }
}

fn get_canonical_path_if_image(entry: &DirEntry) -> Option<String> {
    let file_path = entry.path();
    if !file_path.is_file() {
        return None;
    }

    let extension = file_path.extension()?
        .to_str()?
        .to_lowercase();

    if IMAGE_EXTENSION.contains(&extension.as_str()) {
        fs::canonicalize(file_path)
            .ok()
            .and_then(|path_buf| path_buf.to_str().map(|s| s.to_string()))
    } else {
        return None;
    }
}

fn find_images_recursively(
    current_path: &Path,
    paths_accumulator: &mut Vec<String>) -> io::Result<()> {
    if !current_path.is_dir() {
        return Ok(());
    }

    for entry_result in fs::read_dir(current_path)? {
        let entry = entry_result?;
        let path = entry.path();
        
        if path.is_dir() {
            if let Err(e) = find_images_recursively(&path, paths_accumulator) {
                info!("Error accessing subdirectory {:?}: {}", path, e);
            }
        } else {
            if let Some(image_paths) = get_canonical_path_if_image(&entry) {
                paths_accumulator.push(image_paths);
            }
        }
    }
    Ok(())

}
fn find_absolute_image_path(directory_path: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut image_paths = Vec::new();
    find_images_recursively(directory_path, &mut image_paths)?;
    Ok(image_paths)
}

fn get_random_image(directory_path_str: &str) -> String {
    let directory_path = Path::new(directory_path_str);

    if !directory_path.is_dir() {
        info!("Error: Path is not a directory: {}", directory_path_str);
        return String::new();
    }

    let image_paths = match find_absolute_image_path(directory_path) {
        Ok(paths) => paths,
        Err(_e) => {
            info!("No supoorted image found in directory: {}", directory_path_str);
            return String::new();
        }
    };

    let image_count = image_paths.len();
    if image_count == 0 {
        info!("No suported images found in directory: {}", directory_path_str);
        return String::new();
    }

    let random_index = rand::thread_rng().gen_range(0..image_count);

    image_paths[random_index].clone()
}

async fn get_random_art_handler() -> Result<impl IntoResponse, ImageError> {
    let img = ImageReader::open(Path::new(&get_random_image("/mnt/media/Images/Art/"))).map_err(ImageError::IO)?
        .with_guessed_format().map_err(ImageError::IO)?
        .decode().map_err(ImageError::Load)?;

    let thumb = img.thumbnail(720, 720);
    let mut buffer = Cursor::new(Vec::new());
    thumb.write_to(&mut buffer, ImageFormat::Jpeg)
        .map_err(ImageError::Encode)?;

    Ok(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .body(Body::from(buffer.into_inner()))
            .unwrap()
    )
}
