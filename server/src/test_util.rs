use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn capture_logs() -> (
    impl tracing::Subscriber + Send + Sync,
    std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
) {
    #[derive(Clone)]
    struct Writer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = Writer(buffer.clone());
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    (subscriber, buffer)
}

/// A solid-colour PNG, for the paths that take an uploaded image.
pub fn test_png(size: u32, pixel: [u8; 4]) -> Vec<u8> {
    encode_png(&image::RgbaImage::from_pixel(
        size,
        size,
        image::Rgba(pixel),
    ))
}

/// An incompressible PNG, for the paths that care how heavy an upload is — a
/// solid colour deflates to nothing and would measure nothing. Seeded, so a
/// size assertion means the same thing on every run.
pub fn test_noise_png(size: u32) -> Vec<u8> {
    use rand::{Rng, SeedableRng};

    let mut rng = rand::rngs::StdRng::seed_from_u64(0x5eed);
    let mut image = image::RgbaImage::new(size, size);
    for pixel in image.pixels_mut() {
        let [r, g, b] = rng.gen::<[u8; 3]>();
        *pixel = image::Rgba([r, g, b, 255]);
    }
    encode_png(&image)
}

fn encode_png(image: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut out),
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("encode a test PNG");
    out
}

pub fn unique_temp_dir(name: &str) -> PathBuf {
    let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "_onlinerpg_{name}_{}_{counter}",
        std::process::id()
    ))
}

pub fn temp_auth(name: &str) -> (crate::auth::AuthService, PathBuf) {
    let db_path = unique_temp_dir(name).join("auth.db");
    (
        crate::auth::AuthService::new(db_path.clone()).unwrap(),
        db_path,
    )
}

pub async fn serve(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    base
}
