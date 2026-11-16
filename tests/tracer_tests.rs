use elykseer_lib::common::tracer::{LogFn, LogLevel, Tracer};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Helper: build a Tracer that appends "(level) msg" into a shared Vec.
    fn recording_tracer() -> (Tracer, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));

        let make = |label: &'static str, log: Arc<Mutex<Vec<String>>>| {
            Box::new(move |msg: &str| -> Option<()> {
                log.lock().unwrap().push(format!("[{}] {}", label, msg));
                Some(())
            }) as LogFn
        };

        let tracer = Tracer::new(
            make("DEBUG", Arc::clone(&log)),
            make("INFO", Arc::clone(&log)),
            make("WARNING", Arc::clone(&log)),
            make("ERROR", Arc::clone(&log)),
        );

        (tracer, log)
    }

    // ------------------------------------------------------------------
    // null_tracer
    // ------------------------------------------------------------------

    #[test]
    fn null_tracer_returns_some_for_all_levels() {
        let t = Tracer::null_tracer();
        // ignore silently discards messages but returns Some(()) so that
        // conditional_trace / optional_trace continue execution.
        assert_eq!(t.log(LogLevel::Debug, "msg"), Some(()));
        assert_eq!(t.log(LogLevel::Info, "msg"), Some(()));
        assert_eq!(t.log(LogLevel::Warning, "msg"), Some(()));
        assert_eq!(t.log(LogLevel::Error, "msg"), Some(()));
    }

    // ------------------------------------------------------------------
    // log dispatch
    // ------------------------------------------------------------------

    #[test]
    fn log_dispatches_to_correct_function() {
        let (t, recorded) = recording_tracer();

        t.log(LogLevel::Debug, "d");
        t.log(LogLevel::Info, "i");
        t.log(LogLevel::Warning, "w");
        t.log(LogLevel::Error, "e");

        let entries = recorded.lock().unwrap();
        assert_eq!(
            *entries,
            vec!["[DEBUG] d", "[INFO] i", "[WARNING] w", "[ERROR] e",]
        );
    }

    // ------------------------------------------------------------------
    // conditional_trace
    // ------------------------------------------------------------------

    #[test]
    fn conditional_trace_true_branch_runs_and_logs() {
        let (t, recorded) = recording_tracer();

        let result = t.conditional_trace::<u32>(
            true,
            LogLevel::Info,
            Some("going true"),
            || Some(42),
            LogLevel::Warning,
            Some("going false"),
            || Some(0),
        );

        assert_eq!(result, Some(42));
        let entries = recorded.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "[INFO] going true");
    }

    #[test]
    fn conditional_trace_false_branch_runs_and_logs() {
        let (t, recorded) = recording_tracer();

        let result = t.conditional_trace::<u32>(
            false,
            LogLevel::Info,
            Some("going true"),
            || Some(1),
            LogLevel::Warning,
            Some("going false"),
            || Some(99),
        );

        assert_eq!(result, Some(99));
        let entries = recorded.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "[WARNING] going false");
    }

    #[test]
    fn conditional_trace_no_msg_skips_log_and_still_runs() {
        let (t, recorded) = recording_tracer();

        let result = t.conditional_trace::<&str>(
            true,
            LogLevel::Debug,
            None, // no message → no log call
            || Some("silent ok"),
            LogLevel::Error,
            None,
            || None,
        );

        assert_eq!(result, Some("silent ok"));
        assert!(recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn conditional_trace_null_tracer_still_runs_computation() {
        // null_tracer's ignore returns Some(()) so computation still runs
        let t = Tracer::null_tracer();
        let mut ran = false;

        let result = t.conditional_trace::<u32>(
            true,
            LogLevel::Info,
            Some("silently ignored"),
            || {
                ran = true;
                Some(1)
            },
            LogLevel::Info,
            None,
            || Some(2),
        );

        assert_eq!(result, Some(1));
        assert!(ran, "computation should run even with null tracer");
    }

    // ------------------------------------------------------------------
    // optional_trace
    // ------------------------------------------------------------------

    #[test]
    fn optional_trace_none_branch_runs_and_logs() {
        let (t, recorded) = recording_tracer();

        let result = t.optional_trace::<u32, u32>(
            None,
            LogLevel::Warning,
            Some("got none"),
            || Some(0),
            LogLevel::Info,
            Some("got some"),
            |v| Some(v * 10),
        );

        assert_eq!(result, Some(0));
        let entries = recorded.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "[WARNING] got none");
    }

    #[test]
    fn optional_trace_some_branch_receives_value_and_logs() {
        let (t, recorded) = recording_tracer();

        let result = t.optional_trace::<u32, u32>(
            Some(7),
            LogLevel::Warning,
            Some("got none"),
            || Some(0),
            LogLevel::Info,
            Some("got some"),
            |v| Some(v * 10),
        );

        assert_eq!(result, Some(70));
        let entries = recorded.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "[INFO] got some");
    }

    #[test]
    fn optional_trace_null_tracer_still_runs_computation() {
        let t = Tracer::null_tracer();
        let mut ran = false;

        let result = t.optional_trace::<u32, u32>(
            Some(5),
            LogLevel::Debug,
            None,
            || Some(0),
            LogLevel::Info,
            Some("silently ignored"),
            |_| {
                ran = true;
                Some(99)
            },
        );

        assert_eq!(result, Some(99));
        assert!(ran);
    }
}
