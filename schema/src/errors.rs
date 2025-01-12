use crate::{ErrorInfo, HashClear, RgResult};
use crate::structs::ResponseMetadata;

impl ErrorInfo {
    pub fn error_info<S: Into<String>>(message: S) -> ErrorInfo {
        crate::error_info(message)
    }
    pub fn response_metadata(self) -> ResponseMetadata {
        ResponseMetadata {
            success: false,
            error_info: Some(self),
            task_local_details: vec![],
            request_id: None,
            trace_id: None,
        }
    }
    pub fn enhance(self, message: impl Into<String>) -> ErrorInfo {
        let mut e = self;
        e.message = format!("{} {} ", e.message, message.into());
        e
    }
}

impl HashClear for ErrorInfo {
    fn hash_clear(&mut self) {

    }
}

pub trait EnhanceErrorInfo<T> {
    fn add(self, message: impl Into<String>) -> RgResult<T>;
}

impl<T> EnhanceErrorInfo<T> for RgResult<T> {
    fn add(self, message: impl Into<String>) -> RgResult<T> {
        self.map_err(|e| e.enhance(message))
    }
}