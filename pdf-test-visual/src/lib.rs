use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum VisualTestError {
    RasterizerNotFound,
    RasterizationFailed(String),
    GoldenNotFound(PathBuf),
    Mismatch {
        golden: PathBuf,
        actual: PathBuf,
        detail: String,
    },
    IoError(std::io::Error),
    PngError(String),
}

impl fmt::Display for VisualTestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RasterizerNotFound => write!(
                f,
                "No PDF rasterizer found. Install poppler (pdftoppm) or mupdf-tools (mutool)."
            ),
            Self::RasterizationFailed(msg) => write!(f, "Rasterization failed: {msg}"),
            Self::GoldenNotFound(path) => write!(f, "Golden image not found: {}", path.display()),
            Self::Mismatch {
                golden,
                actual,
                detail,
            } => write!(
                f,
                "Image mismatch: {detail}\n  golden: {}\n  actual: {}",
                golden.display(),
                actual.display()
            ),
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::PngError(msg) => write!(f, "PNG error: {msg}"),
        }
    }
}

impl std::error::Error for VisualTestError {}

impl From<std::io::Error> for VisualTestError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

// ---------------------------------------------------------------------------
// Compare mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum CompareMode {
    Exact,
    Ssim { threshold: f64 },
}

impl Default for CompareMode {
    fn default() -> Self {
        Self::Ssim { threshold: 0.98 }
    }
}

// ---------------------------------------------------------------------------
// Rasterizer detection (cached)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rasterizer {
    Pdftoppm,
    Mutool,
}

fn detect_rasterizer() -> Option<Rasterizer> {
    static RASTERIZER: OnceLock<Option<Rasterizer>> = OnceLock::new();
    *RASTERIZER.get_or_init(|| {
        if Command::new("pdftoppm")
            .arg("-v")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(Rasterizer::Pdftoppm)
        } else if Command::new("mutool")
            .arg("-v")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(Rasterizer::Mutool)
        } else {
            None
        }
    })
}

/// Returns `true` if a PDF rasterizer is available on this system.
pub fn rasterizer_available() -> bool {
    detect_rasterizer().is_some()
}

// ---------------------------------------------------------------------------
// Rasterization
// ---------------------------------------------------------------------------

/// Rasterize a single page of a PDF to PNG bytes.
///
/// `page` is 1-indexed. `dpi` defaults to 150 if set to 0.
pub fn rasterize_page(
    pdf_path: &Path,
    page: u32,
    dpi: u32,
) -> Result<Vec<u8>, VisualTestError> {
    rasterize_page_impl(pdf_path, page, dpi, None)
}

/// Like [`rasterize_page`], but supplies a user password for encrypted PDFs.
pub fn rasterize_page_with_password(
    pdf_path: &Path,
    page: u32,
    dpi: u32,
    password: &str,
) -> Result<Vec<u8>, VisualTestError> {
    rasterize_page_impl(pdf_path, page, dpi, Some(password))
}

fn rasterize_page_impl(
    pdf_path: &Path,
    page: u32,
    dpi: u32,
    password: Option<&str>,
) -> Result<Vec<u8>, VisualTestError> {
    let rasterizer = detect_rasterizer().ok_or(VisualTestError::RasterizerNotFound)?;
    let dpi = if dpi == 0 { 150 } else { dpi };
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = std::env::temp_dir().join(format!("pdf_test_visual_{}_{id}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)?;

    let result = match rasterizer {
        Rasterizer::Pdftoppm => rasterize_pdftoppm(pdf_path, page, dpi, &tmp_dir, password),
        Rasterizer::Mutool => rasterize_mutool(pdf_path, page, dpi, &tmp_dir, password),
    };

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

/// Rasterize all pages of a PDF to PNG bytes. Returns one `Vec<u8>` per page.
pub fn rasterize_all_pages(
    pdf_path: &Path,
    dpi: u32,
) -> Result<Vec<Vec<u8>>, VisualTestError> {
    let page_count = count_pages(pdf_path, None)?;
    let mut pages = Vec::with_capacity(page_count as usize);
    for p in 1..=page_count {
        pages.push(rasterize_page(pdf_path, p, dpi)?);
    }
    Ok(pages)
}

fn count_pages(pdf_path: &Path, password: Option<&str>) -> Result<u32, VisualTestError> {
    let rasterizer = detect_rasterizer().ok_or(VisualTestError::RasterizerNotFound)?;
    match rasterizer {
        Rasterizer::Pdftoppm => {
            let mut cmd = Command::new("pdfinfo");
            if let Some(pw) = password {
                cmd.args(["-upw", pw]);
            }
            let output = cmd
                .arg(pdf_path)
                .output()
                .map_err(|e| VisualTestError::RasterizationFailed(format!("pdfinfo failed: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(rest) = line.strip_prefix("Pages:") {
                    return rest
                        .trim()
                        .parse::<u32>()
                        .map_err(|e| VisualTestError::RasterizationFailed(format!("bad page count: {e}")));
                }
            }
            Err(VisualTestError::RasterizationFailed(
                "could not determine page count".into(),
            ))
        }
        Rasterizer::Mutool => {
            let mut cmd = Command::new("mutool");
            cmd.arg("info");
            if let Some(pw) = password {
                cmd.args(["-p", pw]);
            }
            cmd.arg(pdf_path.to_str().unwrap_or(""));
            let output = cmd
                .output()
                .map_err(|e| VisualTestError::RasterizationFailed(format!("mutool info failed: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(rest) = line.strip_prefix("Pages:") {
                    return rest
                        .trim()
                        .parse::<u32>()
                        .map_err(|e| VisualTestError::RasterizationFailed(format!("bad page count: {e}")));
                }
            }
            Err(VisualTestError::RasterizationFailed(
                "could not determine page count".into(),
            ))
        }
    }
}

fn run_with_timeout(cmd: &mut Command, timeout_secs: u64) -> Result<std::process::Output, VisualTestError> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| VisualTestError::RasterizationFailed(format!("spawn failed: {e}")))?;

    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child.stdout.take().map(|mut s| {
                    let mut buf = Vec::new();
                    std::io::Read::read_to_end(&mut s, &mut buf).ok();
                    buf
                }).unwrap_or_default();
                let stderr = child.stderr.take().map(|mut s| {
                    let mut buf = Vec::new();
                    std::io::Read::read_to_end(&mut s, &mut buf).ok();
                    buf
                }).unwrap_or_default();
                return Ok(std::process::Output { status, stdout, stderr });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(VisualTestError::RasterizationFailed(
                        format!("timed out after {timeout_secs}s"),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                return Err(VisualTestError::RasterizationFailed(format!("wait failed: {e}")));
            }
        }
    }
}

fn rasterize_pdftoppm(
    pdf_path: &Path,
    page: u32,
    dpi: u32,
    tmp_dir: &Path,
    password: Option<&str>,
) -> Result<Vec<u8>, VisualTestError> {
    let prefix = tmp_dir.join("page");
    let mut cmd = Command::new("pdftoppm");
    cmd.args(["-png", "-r", &dpi.to_string(), "-f", &page.to_string(), "-l", &page.to_string()]);
    if let Some(pw) = password {
        cmd.args(["-upw", pw]);
    }
    let output = run_with_timeout(
        cmd.arg(pdf_path).arg(&prefix),
        30,
    )?;

    if !output.status.success() {
        return Err(VisualTestError::RasterizationFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    // pdftoppm names output as {prefix}-{page_number}.png
    // The page number is zero-padded to match the total page count
    let pattern = format!("page-");
    let mut candidates: Vec<_> = std::fs::read_dir(tmp_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(&pattern) && n.ends_with(".png"))
        })
        .collect();
    candidates.sort_by_key(|e| e.file_name());

    if let Some(entry) = candidates.first() {
        Ok(std::fs::read(entry.path())?)
    } else {
        Err(VisualTestError::RasterizationFailed(
            "pdftoppm produced no output file".into(),
        ))
    }
}

fn rasterize_mutool(
    pdf_path: &Path,
    page: u32,
    dpi: u32,
    tmp_dir: &Path,
    password: Option<&str>,
) -> Result<Vec<u8>, VisualTestError> {
    let out_path = tmp_dir.join("page.png");
    let mut cmd = Command::new("mutool");
    cmd.args([
        "draw",
        "-r",
        &dpi.to_string(),
        "-o",
        out_path.to_str().unwrap_or("page.png"),
    ]);
    if let Some(pw) = password {
        cmd.args(["-p", pw]);
    }
    cmd.args([pdf_path.to_str().unwrap_or(""), &page.to_string()]);
    let output = run_with_timeout(
        &mut cmd,
        30,
    )?;

    if !output.status.success() {
        return Err(VisualTestError::RasterizationFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    Ok(std::fs::read(&out_path)?)
}

// ---------------------------------------------------------------------------
// PNG decode helper
// ---------------------------------------------------------------------------

struct DecodedImage {
    width: u32,
    height: u32,
    /// RGBA pixels, 4 bytes per pixel
    rgba: Vec<u8>,
}

fn decode_png(png_bytes: &[u8]) -> Result<DecodedImage, VisualTestError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| VisualTestError::PngError(format!("decode header: {e}")))?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| VisualTestError::PngError(format!("decode frame: {e}")))?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for chunk in buf.chunks_exact(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for chunk in buf.chunks_exact(2) {
                rgba.push(chunk[0]);
                rgba.push(chunk[0]);
                rgba.push(chunk[0]);
                rgba.push(chunk[1]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for &g in &buf {
                rgba.push(g);
                rgba.push(g);
                rgba.push(g);
                rgba.push(255);
            }
            rgba
        }
        other => {
            return Err(VisualTestError::PngError(format!(
                "unsupported color type: {other:?}"
            )));
        }
    };

    Ok(DecodedImage {
        width: info.width,
        height: info.height,
        rgba,
    })
}

#[allow(dead_code)]
fn encode_png(img: &DecodedImage) -> Result<Vec<u8>, VisualTestError> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, img.width, img.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| VisualTestError::PngError(format!("encode header: {e}")))?;
        writer
            .write_image_data(&img.rgba)
            .map_err(|e| VisualTestError::PngError(format!("encode data: {e}")))?;
    }
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Write actual image next to golden for diffing
// ---------------------------------------------------------------------------

fn actual_path_for(golden_path: &Path) -> PathBuf {
    let stem = golden_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = golden_path
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    golden_path.with_file_name(format!("{stem}-actual.{ext}"))
}

fn write_actual(golden_path: &Path, actual_png: &[u8]) -> Result<PathBuf, VisualTestError> {
    let path = actual_path_for(golden_path);
    let mut f = std::fs::File::create(&path)?;
    f.write_all(actual_png)?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Golden image management
// ---------------------------------------------------------------------------

fn should_update_golden() -> bool {
    std::env::var("PDF_TEST_UPDATE_GOLDEN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// If golden doesn't exist, create it and return Ok (first-run behavior).
/// If `PDF_TEST_UPDATE_GOLDEN=1`, overwrite golden and return Ok.
/// Otherwise return None — caller should proceed with comparison.
fn handle_golden_creation(
    golden_path: &Path,
    actual_png: &[u8],
) -> Result<Option<()>, VisualTestError> {
    let update = should_update_golden();

    if !golden_path.exists() {
        if let Some(parent) = golden_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(golden_path, actual_png)?;
        eprintln!(
            "[pdf-test-visual] Created golden image: {}",
            golden_path.display()
        );
        return Ok(Some(()));
    }

    if update {
        std::fs::write(golden_path, actual_png)?;
        eprintln!(
            "[pdf-test-visual] Updated golden image: {}",
            golden_path.display()
        );
        return Ok(Some(()));
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Image comparison: exact
// ---------------------------------------------------------------------------

/// Compare actual PNG bytes against a golden PNG file using exact pixel comparison.
///
/// On mismatch, writes the actual image next to the golden file with an `-actual` suffix.
/// On first run (golden missing), creates the golden image and passes.
pub fn assert_images_exact(
    golden_path: &Path,
    actual_png: &[u8],
) -> Result<(), VisualTestError> {
    if let Some(()) = handle_golden_creation(golden_path, actual_png)? {
        return Ok(());
    }

    let golden_bytes = std::fs::read(golden_path)?;
    let golden = decode_png(&golden_bytes)?;
    let actual = decode_png(actual_png)?;

    if golden.width != actual.width || golden.height != actual.height {
        let actual_file = write_actual(golden_path, actual_png)?;
        return Err(VisualTestError::Mismatch {
            golden: golden_path.to_path_buf(),
            actual: actual_file,
            detail: format!(
                "dimensions differ: golden {}x{}, actual {}x{}",
                golden.width, golden.height, actual.width, actual.height
            ),
        });
    }

    if golden.rgba != actual.rgba {
        let actual_file = write_actual(golden_path, actual_png)?;
        let diff_count = golden
            .rgba
            .iter()
            .zip(actual.rgba.iter())
            .filter(|(a, b)| a != b)
            .count();
        let total = golden.rgba.len();
        return Err(VisualTestError::Mismatch {
            golden: golden_path.to_path_buf(),
            actual: actual_file,
            detail: format!("{diff_count}/{total} bytes differ"),
        });
    }

    // Clean up any stale actual file from previous failures
    let stale = actual_path_for(golden_path);
    if stale.exists() {
        let _ = std::fs::remove_file(&stale);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Image comparison: SSIM
// ---------------------------------------------------------------------------

/// Compare actual PNG bytes against a golden PNG file using SSIM.
///
/// `threshold` is in SSIM scale (1.0 = identical). The `dssim-core` crate returns
/// a *dissimilarity* score where 0 = identical. We convert: `ssim = 1 / (1 + dssim)`.
///
/// On mismatch, writes the actual image next to the golden file with an `-actual` suffix.
/// On first run (golden missing), creates the golden image and passes.
pub fn assert_images_ssim(
    golden_path: &Path,
    actual_png: &[u8],
    threshold: f64,
) -> Result<(), VisualTestError> {
    if let Some(()) = handle_golden_creation(golden_path, actual_png)? {
        return Ok(());
    }

    let golden_bytes = std::fs::read(golden_path)?;
    let golden = decode_png(&golden_bytes)?;
    let actual = decode_png(actual_png)?;

    if golden.width != actual.width || golden.height != actual.height {
        let actual_file = write_actual(golden_path, actual_png)?;
        return Err(VisualTestError::Mismatch {
            golden: golden_path.to_path_buf(),
            actual: actual_file,
            detail: format!(
                "dimensions differ: golden {}x{}, actual {}x{}",
                golden.width, golden.height, actual.width, actual.height
            ),
        });
    }

    let attr = dssim_core::Dssim::new();

    let golden_pixels: Vec<rgb::RGBA8> = golden
        .rgba
        .chunks_exact(4)
        .map(|c| rgb::RGBA8::new(c[0], c[1], c[2], c[3]))
        .collect();
    let actual_pixels: Vec<rgb::RGBA8> = actual
        .rgba
        .chunks_exact(4)
        .map(|c| rgb::RGBA8::new(c[0], c[1], c[2], c[3]))
        .collect();

    let golden_img = attr
        .create_image_rgba(&golden_pixels, golden.width as usize, golden.height as usize)
        .ok_or_else(|| VisualTestError::PngError("failed to create DSSIM golden image".into()))?;
    let actual_img = attr
        .create_image_rgba(&actual_pixels, actual.width as usize, actual.height as usize)
        .ok_or_else(|| VisualTestError::PngError("failed to create DSSIM actual image".into()))?;

    let (dssim_val, _) = attr.compare(&golden_img, &actual_img);
    let dssim: f64 = dssim_val.into();
    // Convert dssim (0 = identical) to ssim-like score (1 = identical)
    let ssim = 1.0 / (1.0 + dssim);

    if ssim < threshold {
        let actual_file = write_actual(golden_path, actual_png)?;
        return Err(VisualTestError::Mismatch {
            golden: golden_path.to_path_buf(),
            actual: actual_file,
            detail: format!("SSIM {ssim:.6} below threshold {threshold:.6} (dssim={dssim:.6})"),
        });
    }

    // Clean up any stale actual file from previous failures
    let stale = actual_path_for(golden_path);
    if stale.exists() {
        let _ = std::fs::remove_file(&stale);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// High-level helpers
// ---------------------------------------------------------------------------

/// Rasterize page `page` (1-indexed) of a PDF and compare against a golden PNG.
///
/// If the golden file doesn't exist, it is created (first-run behavior).
pub fn assert_page_matches(
    pdf_path: &Path,
    page: u32,
    golden_path: &Path,
    mode: CompareMode,
) -> Result<(), VisualTestError> {
    let actual_png = rasterize_page(pdf_path, page, 150)?;
    match mode {
        CompareMode::Exact => assert_images_exact(golden_path, &actual_png),
        CompareMode::Ssim { threshold } => assert_images_ssim(golden_path, &actual_png, threshold),
    }
}

/// Rasterize all pages of a PDF and compare each against golden PNGs in `golden_dir`.
///
/// Golden files are expected at `{golden_dir}/{prefix}-page-{N}.png` (1-indexed).
pub fn assert_all_pages_match(
    pdf_path: &Path,
    golden_dir: &Path,
    prefix: &str,
    mode: CompareMode,
) -> Result<(), VisualTestError> {
    let pages = rasterize_all_pages(pdf_path, 150)?;
    for (i, page_png) in pages.iter().enumerate() {
        let page_num = i + 1;
        let golden_path = golden_dir.join(format!("{prefix}-page-{page_num}.png"));
        match mode {
            CompareMode::Exact => assert_images_exact(&golden_path, page_png)?,
            CompareMode::Ssim { threshold } => {
                assert_images_ssim(&golden_path, page_png, threshold)?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterizer_detection() {
        // Just verify it doesn't panic; result depends on system
        let _ = detect_rasterizer();
    }

    #[test]
    fn test_actual_path_for() {
        let golden = PathBuf::from("/tmp/golden/test.png");
        let actual = actual_path_for(&golden);
        assert_eq!(actual, PathBuf::from("/tmp/golden/test-actual.png"));
    }

    #[test]
    fn test_compare_mode_default() {
        match CompareMode::default() {
            CompareMode::Ssim { threshold } => {
                assert!((threshold - 0.98).abs() < f64::EPSILON);
            }
            CompareMode::Exact => panic!("default should be Ssim"),
        }
    }

    #[test]
    fn test_decode_encode_roundtrip() {
        // Create a minimal 2x2 RGBA PNG
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            // 4 pixels * 4 bytes = 16 bytes
            let data: [u8; 16] = [
                255, 0, 0, 255, // red
                0, 255, 0, 255, // green
                0, 0, 255, 255, // blue
                255, 255, 0, 255, // yellow
            ];
            writer.write_image_data(&data).unwrap();
        }

        let decoded = decode_png(&buf).unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.rgba.len(), 16);
        assert_eq!(decoded.rgba[0..4], [255, 0, 0, 255]); // red

        let re_encoded = encode_png(&decoded).unwrap();
        let decoded2 = decode_png(&re_encoded).unwrap();
        assert_eq!(decoded.rgba, decoded2.rgba);
    }

    #[test]
    fn test_exact_match_identical() {
        let tmp = std::env::temp_dir().join("pdf_test_visual_exact_test");
        let _ = std::fs::create_dir_all(&tmp);
        let golden_path = tmp.join("identical.png");
        let _ = std::fs::remove_file(&golden_path);

        // Create a small PNG
        let png_bytes = create_test_png(2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255]);

        // First run: creates golden
        assert_images_exact(&golden_path, &png_bytes).unwrap();
        assert!(golden_path.exists());

        // Second run: should match
        assert_images_exact(&golden_path, &png_bytes).unwrap();

        let _ = std::fs::remove_file(&golden_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_exact_match_mismatch() {
        let tmp = std::env::temp_dir().join("pdf_test_visual_mismatch_test");
        let _ = std::fs::create_dir_all(&tmp);
        let golden_path = tmp.join("mismatch.png");
        let _ = std::fs::remove_file(&golden_path);

        let png1 = create_test_png(2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255]);
        let png2 = create_test_png(2, 2, &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]);

        // Create golden
        assert_images_exact(&golden_path, &png1).unwrap();

        // Compare with different image
        let err = assert_images_exact(&golden_path, &png2).unwrap_err();
        assert!(matches!(err, VisualTestError::Mismatch { .. }));

        let _ = std::fs::remove_file(&golden_path);
        let actual = tmp.join("mismatch-actual.png");
        let _ = std::fs::remove_file(&actual);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_ssim_identical() {
        let tmp = std::env::temp_dir().join("pdf_test_visual_ssim_test");
        let _ = std::fs::create_dir_all(&tmp);
        let golden_path = tmp.join("ssim_identical.png");
        let _ = std::fs::remove_file(&golden_path);

        let png_bytes = create_test_png(4, 4, &[128u8; 64]);

        // First run creates golden
        assert_images_ssim(&golden_path, &png_bytes, 0.98).unwrap();

        // Second run should match perfectly
        assert_images_ssim(&golden_path, &png_bytes, 0.98).unwrap();

        let _ = std::fs::remove_file(&golden_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn create_test_png(width: u32, height: u32, rgba_data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(rgba_data).unwrap();
        }
        buf
    }
}
