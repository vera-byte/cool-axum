//! 按大小轮转的日志文件 Writer
//!
//! `cool-axum.log` → 写满 `max_size` 后轮转为 `.1` `.2` …，保留 `max_files` 份。

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct RotatingFile {
    path: PathBuf,
    max_size: u64,
    max_files: usize,
    state: Mutex<Inner>,
}

struct Inner {
    /// 当前句柄；轮转时先 `take` 关闭再 rename
    file: Option<File>,
    written: u64,
}

impl RotatingFile {
    pub fn open(path: impl AsRef<Path>, max_size: u64, max_files: usize) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)?;
        }
        let file = open_append(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            max_size: max_size.max(1024),
            max_files: max_files.max(1),
            state: Mutex::new(Inner {
                file: Some(file),
                written,
            }),
        })
    }

    fn rotate_locked(inner: &mut Inner, path: &Path, max_files: usize) -> io::Result<()> {
        if let Some(mut f) = inner.file.take() {
            let _ = f.flush();
            drop(f); // 关闭句柄，确保 rename 成功
        }

        // app.log -> app.log.1 -> app.log.2 ...
        if max_files > 1 {
            let oldest = suffix_path(path, max_files - 1);
            let _ = std::fs::remove_file(&oldest);
            for i in (1..max_files - 1).rev() {
                let from = suffix_path(path, i);
                let to = suffix_path(path, i + 1);
                if from.exists() {
                    let _ = std::fs::rename(&from, &to);
                }
            }
            let _ = std::fs::rename(path, suffix_path(path, 1));
        } else {
            let _ = std::fs::remove_file(path);
        }

        inner.file = Some(open_append(path)?);
        inner.written = 0;
        Ok(())
    }
}

/// 供 tracing 多线程共享（本地 newtype，满足 orphan rule）
#[derive(Clone)]
pub struct SharedFile(pub std::sync::Arc<RotatingFile>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedFile {
    type Writer = RotatingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        RotatingWriter(self.0.clone())
    }
}

pub struct RotatingWriter(std::sync::Arc<RotatingFile>);

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self
            .0
            .state
            .lock()
            .map_err(|_| io::Error::other("log lock"))?;
        if inner.written + buf.len() as u64 > self.0.max_size {
            RotatingFile::rotate_locked(&mut inner, &self.0.path, self.0.max_files)?;
        }
        let file = inner.file.as_mut().ok_or_else(|| io::Error::other("log closed"))?;
        let n = file.write(buf)?;
        inner.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .0
            .state
            .lock()
            .map_err(|_| io::Error::other("log lock"))?;
        if let Some(f) = inner.file.as_mut() {
            f.flush()?;
        }
        Ok(())
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn suffix_path(path: &Path, n: usize) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_when_full() {
        let dir = std::env::temp_dir().join(format!("log-rot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("t.log");
        let w = std::sync::Arc::new(RotatingFile::open(&path, 1024, 3).unwrap());
        let mut f = RotatingWriter(w.clone());
        for i in 0..80 {
            writeln!(f, "line-{i} {}", "x".repeat(40)).unwrap();
        }
        assert!(path.exists(), "当前日志文件应存在");
        assert!(
            suffix_path(&path, 1).exists(),
            "应产生 .1 轮转文件"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
