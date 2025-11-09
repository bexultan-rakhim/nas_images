use axum::{
    body::Body,
    extract::State,
    http::{header, Response, StatusCode},
    response::{IntoResponse, Response as AxumResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use image::{ImageReader, ImageFormat};

use std::io::{self, Cursor};
use std::path::Path;
use std::fs::{self, File, DirEntry};

use rand::Rng;

use log::{info, error, LevelFilter};
use simplelog::{CombinedLogger, Config, WriteLogger};

use serde::Deserialize;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    config: String,
    log: String,
}

const IMAGE_EXTENSION: [&str; 3] = ["png", "jpg", "jpeg"];
#[tokio::main]
async fn main() {
    let args = Args::parse();
    CombinedLogger::init(
        vec![
            WriteLogger::new(
                LevelFilter::Info,
                Config::default(),
                File::create(args.log + "nas_server.log").unwrap()
            ),
        ]
    ).unwrap();
    
    let media_confg = MediaConfig::new(&args.config).unwrap(); 

    match MediaState::new(media_confg) {
        Ok(state) => {
            let addr = state.media_config.network.clone(); 
            info!(" Server started, listening on http://{}", addr);
            let listener = TcpListener::bind(addr).await.unwrap();
            
            let shared_state = Arc::new(state);
            let app = Router::new()
                .route("/get_random_art", get(get_random_art_handler))
                .with_state(shared_state);
            axum::serve(listener, app).await.unwrap();
        }
        Err(e) => error!("Failed to load media {}", e),
    }
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
                error!("{}",error_msg);
                (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
            }
            ImageError::Load(e) => {
                let error_msg = format!("Failed to load image: {}", e);
                error!("{}",error_msg);
                (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
            }
            ImageError::Encode(e) => {
                let error_msg = format!("Failed to encode Image: {}", e);
                error!("{}",error_msg);
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
         None
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
                error!("Error accessing subdirectory {:?}: {}", path, e);
            }
        } else if let Some(image_paths) = get_canonical_path_if_image(&entry) {
            paths_accumulator.push(image_paths);
        }
    }
    Ok(())

}

fn find_absolute_image_path(directory_path: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut image_paths = Vec::new();
    find_images_recursively(directory_path, &mut image_paths)?;
    Ok(image_paths)
}

#[derive(Clone)]
pub struct MediaState {
    media_config: MediaConfig,
    paths: Vec<String>
}

impl MediaState {
    pub fn new(media_config: MediaConfig) -> Result<Self, String> {
        let directory_path = Path::new(&media_config.media);

        if !directory_path.is_dir() {
            return Err(format!("Error: Path is not a directory: {}", &media_config.media));
        }

        match find_absolute_image_path(directory_path) {
            Ok(paths) => if !paths.is_empty() {
                    Ok(MediaState{media_config, paths })
                } else {
                Err(format!("Directory does not contain images: {}", &media_config.media))
            },
            Err(_e) => Err(
                format!("No supoorted image found in directory: {}", &media_config.media)
            )
        }
    }

    pub fn image_count(&self) -> usize {
        self.paths.len()
    }

    pub fn get_random_image(&self) -> &str {
        let image_count = self.image_count();
        let random_index = rand::thread_rng().gen_range(0..image_count);
        &self.paths[random_index]
    }
}

async fn get_random_art_handler(
    State(state): State<Arc<MediaState>>,
) -> Result<impl IntoResponse, ImageError> {
    let img_path = state.get_random_image();
    let img = ImageReader::open(Path::new(img_path)).map_err(ImageError::IO)?
        .with_guessed_format().map_err(ImageError::IO)?
        .decode().map_err(ImageError::Load)?;
    
    let resolution: u32 = state.media_config.image.resolution;
    let thumb = img.thumbnail(
        resolution,
        resolution);
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

#[derive(Debug, Deserialize)]
pub struct NetworkConfigRaw {
    pub addr: [u8; 4], 
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ImageConfig {
    pub resolution: u32,
}

#[derive(Debug, Deserialize)]
pub struct MediaConfigRaw {
    #[serde(rename = "media_dir")]
    pub media: String,
    pub network: NetworkConfigRaw,
    pub image: ImageConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MediaConfig {
    pub media: String,
    pub network: SocketAddr,
    pub image: ImageConfig,
}

impl MediaConfig {
    pub fn new( path: &str) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(
                |e| format!("Could not read config file '{}': {}", path, e))?;
        let raw_config: MediaConfigRaw = toml::from_str(&contents)
            .map_err(
                |e| format!(
                    "Could not parse TOML from file '{}': {}", path, e))?;
        let network_socket = SocketAddr::from(
            (raw_config.network.addr, raw_config.network.port));

        Ok(MediaConfig {
            media: raw_config.media,
            network: network_socket,  
            image: raw_config.image,
        })
    }
}
