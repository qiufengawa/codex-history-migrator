#[derive(Debug, Clone, PartialEq)]
pub struct OperationProgress {
    pub fraction: f32,
    pub message: String,
}

impl OperationProgress {
    pub fn new(fraction: f32, message: impl Into<String>) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            message: message.into(),
        }
    }
}
