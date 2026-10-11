pub mod feedback;
pub mod handlers;
pub mod logging;
pub mod messages;
pub mod rate_limit;
pub mod service;
pub mod smtp;
pub mod subscriber;
pub mod transport;

pub use service::EmailService;
