use crate::model::{Document, ExportSettings};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::JoinHandle,
};

pub struct ExportJob {
    pub sender: Option<SyncSender<Vec<u8>>>,
    pub result: Receiver<Result<PathBuf, String>>,
    pub frame: usize,
    pub frames: usize,
    pub size: [i32; 2],
    pub settings: ExportSettings,
    pub pending: Option<Vec<u8>>,
    pub document: Document,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl ExportJob {
    pub fn start(path: &Path, document: &Document) -> Result<Self, String> {
        let settings = document.settings.export.clone();
        if settings.end <= settings.start || settings.fps == 0 {
            return Err("Choose a valid export range and frame rate".into());
        }
        let width = (settings.width / 2 * 2).max(2);
        let height = ((width as f32 / document.settings.aspect / 2.0).round() as u32 * 2).max(2);
        let frames = ((settings.end - settings.start) * settings.fps as f64).ceil() as usize;
        let parent = path.parent().unwrap_or(Path::new("."));
        // Encode beside the destination, then replace it only after successful completion.
        let temporary = tempfile::Builder::new()
            .suffix(".mp4")
            .tempfile_in(parent)
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(ffmpeg_path());
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &format!("{width}x{height}"),
            "-framerate",
            &settings.fps.to_string(),
            "-i",
            "pipe:0",
        ]);
        let music = if settings.audio {
            if let Some(asset) = &document.audio {
                let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
                file.write_all(&asset.bytes).map_err(|e| e.to_string())?;
                command
                    .args(["-ss", &settings.start.to_string(), "-i"])
                    .arg(file.path());
                command.args([
                    "-map", "0:v:0", "-map", "1:a:0", "-af", "apad", "-c:a", "aac", "-b:a", "256k",
                ]);
                Some(file)
            } else {
                None
            }
        } else {
            None
        };
        command
            .args([
                "-vf",
                "vflip",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                &settings.crf.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-t",
                &(frames as f64 / settings.fps as f64).to_string(),
                "-movflags",
                "+faststart",
            ])
            .arg(temporary.path());
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!("Cannot start FFmpeg ({e}). Install FFmpeg and make it available on PATH.")
            })?;
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(2);
        let (result_tx, result) = mpsc::channel();
        let destination = path.to_owned();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = std::thread::spawn(move || {
            let _music = music;
            let mut stderr = child.stderr.take().unwrap();
            let errors = std::thread::spawn(move || {
                let mut text = String::new();
                let _ = stderr.read_to_string(&mut text);
                text
            });
            let mut input = child.stdin.take().unwrap();
            let mut error = None;
            for frame in receiver {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                if let Err(e) = input.write_all(&frame) {
                    error = Some(e.to_string());
                    break;
                }
            }
            drop(input);
            if worker_cancelled.load(Ordering::Acquire) {
                let _ = child.kill();
            }
            let status = child.wait();
            let logs = errors.join().unwrap_or_default();
            let outcome = if worker_cancelled.load(Ordering::Acquire) {
                Err("Export cancelled".into())
            } else {
                match status {
                    Ok(s) if s.success() && error.is_none() => temporary
                        .persist(&destination)
                        .map(|_| destination)
                        .map_err(|e| e.to_string()),
                    _ => Err(format!(
                        "Video export failed: {} {}",
                        error.unwrap_or_default(),
                        logs.trim()
                    )),
                }
            };
            let _ = result_tx.send(outcome);
        });
        Ok(Self {
            sender: Some(sender),
            result,
            frame: 0,
            frames,
            size: [width as i32, height as i32],
            settings,
            pending: None,
            document: document.clone(),
            cancelled,
            worker: Some(worker),
        })
    }
    pub fn time(&self) -> f64 {
        self.settings.start + self.frame as f64 / self.settings.fps as f64
    }
}

fn ffmpeg_path() -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let bundled = parent.join(name);
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    #[cfg(target_os = "macos")]
    for path in ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from(name)
}

impl Drop for ExportJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
