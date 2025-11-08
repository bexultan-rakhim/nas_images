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

use std::io::Cursor;
use std::path::Path;
use std::fs::File;

use log::{info, LevelFilter};
use simplelog::{CombinedLogger, Config, WriteLogger};

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
        .route("/get_image", get(get_image_hander));
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

async fn get_image_hander() -> Result<impl IntoResponse, ImageError> {
    let img = ImageReader::open(Path::new("/mnt/media/Images/Art/limes.png")).map_err(ImageError::IO)?
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
