use std::{env, fs};

use patient_safe_appointment_reporter::{
    appointment_report::{dispatch_report, AppointmentReport},
    infrai_email::EmailClient,
};
use thiserror::Error;

#[derive(Debug, Error)]
enum ReporterError {
    #[error("usage: appointment_reporter <appointment.json>")]
    MissingInput,
    #[error("cannot read appointment input: {0}")]
    Read(#[from] std::io::Error),
    #[error("appointment input is invalid: {0}")]
    Decode(#[from] serde_json::Error),
    #[error(transparent)]
    Email(#[from] patient_safe_appointment_reporter::infrai_email::EmailError),
}

#[tokio::main]
async fn main() -> Result<(), ReporterError> {
    let path = env::args().nth(1).ok_or(ReporterError::MissingInput)?;
    let report: AppointmentReport = serde_json::from_str(&fs::read_to_string(path)?)?;
    let outcome = dispatch_report(&EmailClient::from_env()?, &report).await?;
    println!("{outcome:?}");
    Ok(())
}
