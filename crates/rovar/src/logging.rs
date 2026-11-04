use flexi_logger::{
    Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode,
};
use std::{backtrace::Backtrace, path::PathBuf};

pub(crate) struct Logging(LoggerHandle);

pub(crate) fn init() -> Option<Logging> {
    let directory = match gpui::block_on(directory()) {
        Ok(directory) => directory,
        Err(error) => {
            eprintln!("Could not create Rovar log directory: {error:#}");
            return None;
        }
    };
    let result = Logger::try_with_env_or_str("warn,rovar=info,gpui_wgpu::wgpu_context=info,gpui::glyph_cache=info,gpui_wgpu::glyph_raster=info")
        .map(|logger| {
            logger
                .log_to_file(FileSpec::default().directory(&directory).basename("rovar"))
                .rotate(
                    Criterion::Size(10 * 1024 * 1024),
                    Naming::TimestampsDirect,
                    Cleanup::KeepLogFiles(5),
                )
                .format(flexi_logger::with_thread)
                .duplicate_to_stderr(Duplicate::Warn)
                .write_mode(WriteMode::Async)
        })
        .and_then(Logger::start);
    let handle = match result {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("Could not initialize Rovar logging: {error}");
            return None;
        }
    };
    let previous = std::panic::take_hook();
    let panic_logger = handle.clone();
    std::panic::set_hook(Box::new(move |info| {
        log::error!(target: "rovar::panic", "{info}\n{}", Backtrace::force_capture());
        panic_logger.flush();
        previous(info);
    }));
    log::info!(
        "Starting Rovar {} os={} arch={} pid={} logs={}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::process::id(),
        directory.display()
    );
    Some(Logging(handle))
}

async fn directory() -> anyhow::Result<PathBuf> {
    use anyhow::Context;
    use gpui_io::{FileSystem, IoExecutor, LocationHandle, SystemLocation};

    let executor = IoExecutor::new(|work| {
        let _ = std::thread::Builder::new()
            .name("rovar-log-directory".into())
            .spawn(work);
    });
    let directory = if let Some(path) = std::env::var_os("ROVAR_LOG_DIR") {
        let path = PathBuf::from(path);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        LocationHandle::from_path(parent, executor)
            .create_dir(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .context("Invalid log directory")?,
            )
            .await?;
        path
    } else {
        let root = if let Some(path) = std::env::var_os("ROVAR_DATA_DIR") {
            LocationHandle::from_path(path, executor)
        } else {
            FileSystem::desktop("rovar", executor)?
                .location(SystemLocation::AppData)
                .await?
        };
        root.create_dir("logs").await?;
        root.path()
            .context("Log directory has no native path")?
            .join("logs")
    };
    Ok(directory)
}

impl Drop for Logging {
    fn drop(&mut self) {
        log::info!("Rovar stopped");
        self.0.shutdown();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_logging_rotates_and_retains_thread_panics() {
        if std::env::var_os("ROVAR_LOGGING_TEST_CHILD").is_some() {
            let logging = super::init().expect("logger initialization failed");
            let payload = "x".repeat(8192);
            for index in 0..1400 {
                log::info!("rotation record {index}: {payload}");
            }
            let thread = std::thread::Builder::new()
                .name("logging-panic-probe".into())
                .spawn(|| panic!("background panic captured by the logger"))
                .unwrap();
            assert!(thread.join().is_err());
            log::info!("Logging continued after the worker panic");
            drop(logging);
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "logging::tests::file_logging_rotates_and_retains_thread_panics",
                "--nocapture",
            ])
            .env("ROVAR_LOGGING_TEST_CHILD", "1")
            .env("ROVAR_LOG_DIR", directory.path())
            .env("RUST_LOG", "warn,rovar=info")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let files: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert!((2..=6).contains(&files.len()), "{files:?}");
        let contents = files
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<String>();
        for expected in [
            "logging-panic-probe",
            "background panic captured by the logger",
            "Logging continued after the worker panic",
            "Rovar stopped",
        ] {
            assert!(contents.contains(expected), "Missing {expected}");
        }
    }
}
