#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// A log function; `Send + Sync` so that a `Configuration` can be shared
/// between threads.
pub type LogFn = Box<dyn Fn(&str) -> Option<()> + Send + Sync>;

pub struct Tracer {
    pub log_debug: LogFn,
    pub log_info: LogFn,
    pub log_warning: LogFn,
    pub log_error: LogFn,
}

impl Tracer {
    pub fn new(log_debug: LogFn, log_info: LogFn, log_warning: LogFn, log_error: LogFn) -> Self {
        Tracer {
            log_debug,
            log_info,
            log_warning,
            log_error,
        }
    }

    /// Coq's `ignore`: silently discard the message but signal success
    /// so that `conditional_trace` / `optional_trace` continue execution.
    fn ignore(_: &str) -> Option<()> {
        Some(())
    }

    /// Coq's `output_stdout` axiom: print to stdout and return Some(()).
    pub fn output_stdout(level: LogLevel, msg: &str) -> Option<()> {
        let prefix = match level {
            LogLevel::Debug => "[DEBUG]",
            LogLevel::Info => "[INFO]",
            LogLevel::Warning => "[WARNING]",
            LogLevel::Error => "[ERROR]",
        };
        println!("{} {}", prefix, msg);
        Some(())
    }

    /// nullTracer: all log functions are `ignore`.
    pub fn null_tracer() -> Self {
        Tracer::new(
            Box::new(Self::ignore),
            Box::new(Self::ignore),
            Box::new(Self::ignore),
            Box::new(Self::ignore),
        )
    }

    /// stdoutTracerDebug: all levels go to stdout.
    pub fn stdout_tracer_debug() -> Self {
        Tracer::new(
            Box::new(|m| Self::output_stdout(LogLevel::Debug, m)),
            Box::new(|m| Self::output_stdout(LogLevel::Info, m)),
            Box::new(|m| Self::output_stdout(LogLevel::Warning, m)),
            Box::new(|m| Self::output_stdout(LogLevel::Error, m)),
        )
    }

    /// stdoutTracerInfo: debug is silenced.
    pub fn stdout_tracer_info() -> Self {
        Tracer::new(
            Box::new(Self::ignore),
            Box::new(|m| Self::output_stdout(LogLevel::Info, m)),
            Box::new(|m| Self::output_stdout(LogLevel::Warning, m)),
            Box::new(|m| Self::output_stdout(LogLevel::Error, m)),
        )
    }

    /// stdoutTracerWarning: debug and info are silenced.
    pub fn stdout_tracer_warning() -> Self {
        Tracer::new(
            Box::new(Self::ignore),
            Box::new(Self::ignore),
            Box::new(|m| Self::output_stdout(LogLevel::Warning, m)),
            Box::new(|m| Self::output_stdout(LogLevel::Error, m)),
        )
    }

    /// stdoutTracerError: only errors go to stdout.
    pub fn stdout_tracer_error() -> Self {
        Tracer::new(
            Box::new(Self::ignore),
            Box::new(Self::ignore),
            Box::new(Self::ignore),
            Box::new(|m| Self::output_stdout(LogLevel::Error, m)),
        )
    }

    /// Dispatch a message to the appropriate log function.
    pub fn log(&self, level: LogLevel, msg: &str) -> Option<()> {
        match level {
            LogLevel::Debug => (self.log_debug)(msg),
            LogLevel::Info => (self.log_info)(msg),
            LogLevel::Warning => (self.log_warning)(msg),
            LogLevel::Error => (self.log_error)(msg),
        }
    }

    /// Coq's `conditionalTrace`.
    ///
    /// If `condition` is true, optionally log `true_msg` at `ll_true`, then
    /// run `true_computation`.  The log call returning `None` short-circuits
    /// (mirrors the Coq `match log … | None => None` branch).
    #[allow(clippy::too_many_arguments)]
    pub fn conditional_trace<A>(
        &self,
        condition: bool,
        ll_true: LogLevel,
        true_msg: Option<&str>,
        true_computation: impl FnOnce() -> Option<A>,
        ll_false: LogLevel,
        false_msg: Option<&str>,
        false_computation: impl FnOnce() -> Option<A>,
    ) -> Option<A> {
        if condition {
            if let Some(m) = true_msg {
                self.log(ll_true, m)?; // None => short-circuit
            }
            true_computation()
        } else {
            if let Some(m) = false_msg {
                self.log(ll_false, m)?;
            }
            false_computation()
        }
    }

    /// Coq's `optionalTrace`.
    ///
    /// Branches on whether `computation` is `None` or `Some(b)`, optionally
    /// logs a message, then runs the appropriate continuation.
    #[allow(clippy::too_many_arguments)]
    pub fn optional_trace<A, B>(
        &self,
        computation: Option<B>,
        ll_none: LogLevel,
        none_msg: Option<&str>,
        computation_none: impl FnOnce() -> Option<A>,
        ll_some: LogLevel,
        some_msg: Option<&str>,
        computation_some: impl FnOnce(B) -> Option<A>,
    ) -> Option<A> {
        match computation {
            None => {
                if let Some(m) = none_msg {
                    self.log(ll_none, m)?;
                }
                computation_none()
            }
            Some(b) => {
                if let Some(m) = some_msg {
                    self.log(ll_some, m)?;
                }
                computation_some(b)
            }
        }
    }
}
