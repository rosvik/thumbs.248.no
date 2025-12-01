pub enum LogType {
    Debug,
    Info,
    Warning,
    Error,
}

#[macro_export]
macro_rules! log {
    ($fmt:expr, $log_type:expr $(, $args:expr)* $(,)?) => {{
        let ms = chrono::Local::now().timestamp_subsec_millis();
        let format_str = format!("%Y-%m-%d %H:%M:%S.{}", ms);
        let timestamp = chrono::Local::now().format(&format_str);

        let color = match $log_type {
            LogType::Debug => "\x1b[90m", // gray
            LogType::Info => "\x1b[37m", // white
            LogType::Warning => "\x1b[33m", // yellow
            LogType::Error => "\x1b[31m", // red
        };

        let out: &mut dyn std::io::Write = match $log_type {
            LogType::Debug | LogType::Info => &mut std::io::stdout(),
            LogType::Warning | LogType::Error => &mut std::io::stderr(),
        };

        let _ = writeln!(out, "{color}[{timestamp}] {}\x1b[0m", format!($fmt $(, $args)*));
    }};
}
