//! Copy text out of a screenshot.
//!
//! Windows uses its built-in OCR engine (Windows.Media.Ocr, the one the
//! Snipping Tool uses), so nothing extra is installed. Elsewhere Shot uses the
//! `tesseract` command if it is on the PATH.

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

/// Small captures OCR far better when enlarged first.
fn prepare(pm: &Pixmap) -> Pixmap {
    let k = if pm.height() < 300 && pm.width() < 3000 { 2.0 } else { 1.0 };
    if k == 1.0 {
        return pm.clone();
    }
    let (w, h) = ((pm.width() as f32 * k) as u32, (pm.height() as f32 * k) as u32);
    let Some(mut out) = Pixmap::new(w, h) else { return pm.clone() };
    out.fill(tiny_skia::Color::WHITE);
    let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
    out.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_scale(k, k), None);
    out
}

/// Blocking; call from a worker thread.
pub fn recognize(pm: &Pixmap) -> Result<String, String> {
    let pm = prepare(pm);
    imp::recognize(&pm).map(|t| t.trim_end().to_string())
}

pub fn available() -> bool {
    imp::available()
}

#[cfg(windows)]
mod imp {
    use tiny_skia::Pixmap;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;

    pub fn available() -> bool {
        OcrEngine::TryCreateFromUserProfileLanguages().is_ok()
    }

    pub fn recognize(pm: &Pixmap) -> Result<String, String> {
        // Worker threads need the WinRT runtime; an "already initialised" result is fine.
        let _ = unsafe { windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED) };
        run(pm).map_err(|e| e.message().to_string())
    }

    fn run(pm: &Pixmap) -> windows::core::Result<String> {
        let max = OcrEngine::MaxImageDimension()?;
        if pm.width() > max || pm.height() > max {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80070057u32 as i32),
                format!("image is larger than {max} px, select a smaller area"),
            ));
        }
        let mut bgra = pm.data().to_vec();
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let buffer = writer.DetachBuffer()?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            pm.width() as i32,
            pm.height() as i32,
            BitmapAlphaMode::Premultiplied,
        )?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        let lines = result.Lines()?;
        let mut out = String::new();
        for i in 0..lines.Size()? {
            out.push_str(&lines.GetAt(i)?.Text()?.to_string_lossy());
            out.push('\n');
        }
        Ok(out)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Command;
    use tiny_skia::Pixmap;

    pub fn available() -> bool {
        Command::new("tesseract").arg("--version").output().is_ok_and(|o| o.status.success())
    }

    pub fn recognize(pm: &Pixmap) -> Result<String, String> {
        let path = std::env::temp_dir().join(format!("shot-ocr-{}.png", std::process::id()));
        pm.save_png(&path).map_err(|e| e.to_string())?;
        let out = Command::new("tesseract").arg(&path).arg("stdout").output();
        let _ = std::fs::remove_file(&path);
        let out = out.map_err(|e| format!("tesseract: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
