/// Ensure a condition holds. If it fails, emit a [`RepairPacket`] and panic.
///
/// This is the primary agent-facing assertion macro. Unlike `assert!`, it
/// produces a structured repair packet with error code, message, hints, and
/// commands before panicking.
///
/// # Arguments
///
/// - `$condition` — boolean expression to check
/// - `$code` — stable error code string (e.g., `"PRICE-NEGATIVE"`)
/// - `$message` — human-readable failure description
/// - `$hint` — repair hint for the agent
/// - `$commands` — slice of local validation commands
///
/// # Example
///
/// ```should_panic
/// use witness_rt::agent_ensure;
///
/// let total: i64 = -5;
/// agent_ensure!(
///     total >= 0,
///     "PRICE-NEGATIVE",
///     "total must be non-negative",
///     "check discount logic before computing total",
///     ["cargo test -p pricing"]
/// );
/// ```
#[macro_export]
macro_rules! agent_ensure {
    ($condition:expr, $code:expr, $message:expr, $hint:expr, [$($cmd:expr),* $(,)?]) => {
        if !$condition {
            $crate::emit_and_panic($code, $message.to_string(), $hint, vec![$($cmd.to_string()),*]);
        }
    };
}

/// Emit a [`RepairPacket`] and panic unconditionally.
///
/// Use this when a code path is provably impossible under the cell's
/// invariants but you want to leave a structured repair trail.
///
/// # Example
///
/// ```should_panic
/// use witness_rt::agent_bail;
///
/// agent_bail!(
///     "UNREACHABLE-STATE",
///     "entered impossible branch in parser",
///     "check state machine transitions"
/// );
/// ```
#[macro_export]
macro_rules! agent_bail {
    ($code:expr, $message:expr, $hint:expr) => {{
        $crate::emit_and_panic($code, $message.to_string(), $hint, vec![])
    }};
}

/// Unwrap an `Option`, emitting a [`RepairPacket`] on `None`.
///
/// This is a structured replacement for `.expect()`. Instead of a bare
/// panic message, the agent gets a repair packet with code, hint, and
/// source location.
///
/// # Example
///
/// ```should_panic
/// use witness_rt::agent_expect;
///
/// let value: Option<i32> = None;
/// let _result = agent_expect!(
///     value,
///     "CONFIG-MISSING",
///     "expected configuration value to be present",
///     "check .env or config deserialization"
/// );
/// ```
#[macro_export]
macro_rules! agent_expect {
    ($option:expr, $code:expr, $message:expr, $hint:expr) => {
        match $option {
            Some(value) => value,
            None => $crate::emit_and_panic($code, $message.to_string(), $hint, vec![]),
        }
    };
}

/// Unwrap a `Result`, emitting a [`RepairPacket`] on `Err`.
///
/// The error value is included in the packet message. This is a structured
/// replacement for `.unwrap()`.
///
/// # Example
///
/// ```should_panic
/// use witness_rt::agent_ok;
///
/// let result: Result<i32, &str> = Err("parse failed");
/// let _value = agent_ok!(
///     result,
///     "PARSE-FAILED",
///     "expected numeric value",
///     "check input format"
/// );
/// ```
#[macro_export]
macro_rules! agent_ok {
    ($result:expr, $code:expr, $message:expr, $hint:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => $crate::emit_and_panic(
                $code,
                format!("{}: {error}", $message),
                $hint,
                vec![],
            ),
        }
    };
}
