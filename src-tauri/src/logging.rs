use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use chrono::Local;
use lazy_static::lazy_static;
use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};

// Global file loggers
lazy_static! {
    static ref DEBUG_LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
    static ref ERROR_LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
}

struct TauriLogger;

impl log::Log for TauriLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
            let log_message = format!(
                "[{}] {} [{}:{}] {}\n", 
                timestamp, 
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            );
            
            // Print to console
            println!("{}", log_message.trim());
            
            // Log to file based on level
            match record.level() {
                Level::Error | Level::Warn => {
                    if let Some(file) = &mut *ERROR_LOG_FILE.lock().unwrap() {
                        let _ = file.write_all(log_message.as_bytes());
                        let _ = file.flush();
                    }
                },
                _ => {}
            }
            
            // Always write to debug log
            if let Some(file) = &mut *DEBUG_LOG_FILE.lock().unwrap() {
                let _ = file.write_all(log_message.as_bytes());
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Some(file) = &mut *DEBUG_LOG_FILE.lock().unwrap() {
            let _ = file.flush();
        }
        if let Some(file) = &mut *ERROR_LOG_FILE.lock().unwrap() {
            let _ = file.flush();
        }
    }
}

// Dummy logger needed for error handling
static DUMMY_LOGGER: DummyLogger = DummyLogger;

struct DummyLogger;

impl log::Log for DummyLogger {
    fn enabled(&self, _: &Metadata) -> bool { false }
    fn log(&self, _: &Record) {}
    fn flush(&self) {}
}

// This is the fixed function - we need to unwrap the Result to get SetLoggerError
fn file_error<E>(e: E) -> SetLoggerError 
where 
    E: std::fmt::Display 
{
    eprintln!("Logging error: {}", e);
    // Create a SetLoggerError by attempting to set a logger when one is already set
    match log::set_logger(&DUMMY_LOGGER) {
        Ok(_) => panic!("Unexpected success setting dummy logger"),
        Err(e) => e  // This returns the SetLoggerError we need
    }
}

pub fn init(app_name: &str) -> Result<(), SetLoggerError> {
    // Create logs directory
    let log_dir = match get_log_directory(app_name) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("Failed to create log directory: {}", e);
            return Err(file_error(e));
        }
    };
    
    // Initialize log files
    let debug_log_path = log_dir.join("debug.log");
    let error_log_path = log_dir.join("error.log");
    
    // Open log files
    let debug_file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(debug_log_path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to open debug log file: {}", e);
                return Err(file_error(e));
            }
        };
    
    let error_file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(error_log_path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to open error log file: {}", e);
                return Err(file_error(e));
            }
        };
    
    // Store file handles
    *DEBUG_LOG_FILE.lock().unwrap() = Some(debug_file);
    *ERROR_LOG_FILE.lock().unwrap() = Some(error_file);
    
    // Set the logger
    log::set_boxed_logger(Box::new(TauriLogger))?;
    
    // Set maximum log level
    #[cfg(debug_assertions)]
    log::set_max_level(LevelFilter::Debug);
    
    #[cfg(not(debug_assertions))]
    log::set_max_level(LevelFilter::Info);
    
    Ok(())
}

fn get_log_directory(app_name: &str) -> Result<PathBuf, std::io::Error> {
    let mut log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    
    log_dir.push(app_name);
    log_dir.push("logs");
    
    // Create directory if it doesn't exist
    fs::create_dir_all(&log_dir)?;
    
    Ok(log_dir)
}

pub fn get_log_paths(app_name: &str) -> Option<(PathBuf, PathBuf)> {
    if let Ok(log_dir) = get_log_directory(app_name) {
        let debug_log_path = log_dir.join("debug.log");
        let error_log_path = log_dir.join("error.log");
        Some((debug_log_path, error_log_path))
    } else {
        None
    }
}

// Clean up old log entries to prevent files from growing too large
pub fn rotate_logs(app_name: &str, max_size_mb: u64) -> Result<(), String> {
    if let Some((debug_path, error_path)) = get_log_paths(app_name) {
        rotate_log_file(debug_path, max_size_mb)?;
        rotate_log_file(error_path, max_size_mb)?;
    }
    Ok(())
}

fn rotate_log_file(path: PathBuf, max_size_mb: u64) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    
    let metadata = fs::metadata(&path).map_err(|e| format!("Failed to get metadata: {}", e))?;
    let size_mb = metadata.len() / (1024 * 1024);
    
    if size_mb > max_size_mb {
        let backup_path = path.with_extension("log.old");
        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|e| format!("Failed to remove old backup: {}", e))?;
        }
        
        // Rename current log to backup
        fs::rename(&path, &backup_path).map_err(|e| format!("Failed to rotate log: {}", e))?;
        
        // Create new empty log file
        File::create(&path).map_err(|e| format!("Failed to create new log file: {}", e))?;
    }
    
    Ok(())
}
