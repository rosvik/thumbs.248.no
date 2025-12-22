#[derive(PartialEq)]
pub enum LogType {
    Debug,
    Info,
    Warning,
    Error,
    Performance,
}

#[macro_export]
macro_rules! log {
    ($fmt:expr, $log_type:expr $(, $args:expr)* $(,)?) => {{
        let ms = chrono::Local::now().timestamp_subsec_millis();
        let format_str = format!("%Y-%m-%d %H:%M:%S.{:03}", ms);
        let timestamp = chrono::Local::now().format(&format_str);

        let color = match $log_type {
            LogType::Debug => "\x1b[90m", // gray
            LogType::Info => "\x1b[37m", // white
            LogType::Warning => "\x1b[33m", // yellow
            LogType::Error => "\x1b[31m", // red
            LogType::Performance => "\x1b[32m", // green
        };

        let out: &mut dyn std::io::Write = match $log_type {
            LogType::Debug | LogType::Info | LogType::Performance => &mut std::io::stdout(),
            LogType::Warning | LogType::Error => &mut std::io::stderr(),
        };

        // Only print performance logs if DEBUG is set to true
        if $log_type != LogType::Performance || std::env::var("DEBUG").unwrap_or_default() == "true"{
            let _ = writeln!(out, "{color}[{timestamp}] {}\x1b[0m", format!($fmt $(, $args)*));
        }
    }};
}
