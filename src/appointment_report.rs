use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;

use crate::infrai_email::{EmailClient, EmailError, SendReceipt};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentStatus {
    Scheduled,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppointmentReport {
    pub appointment_id: String,
    pub patient_email: String,
    pub occurred_at: String,
    pub status: AppointmentStatus,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DispatchDecision {
    Send,
    Skip { reason: &'static str },
}

impl AppointmentReport {
    pub fn dispatch_decision(&self) -> DispatchDecision {
        match self.status {
            AppointmentStatus::Completed => DispatchDecision::Send,
            AppointmentStatus::Scheduled => DispatchDecision::Skip {
                reason: "appointment has not completed",
            },
            AppointmentStatus::Cancelled => DispatchDecision::Skip {
                reason: "cancelled appointments have no report",
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    Sent { message_id: String },
    Skipped { reason: &'static str },
}

pub async fn dispatch_report(
    client: &EmailClient,
    report: &AppointmentReport,
) -> Result<DispatchOutcome, EmailError> {
    if let DispatchDecision::Skip { reason } = report.dispatch_decision() {
        return Ok(DispatchOutcome::Skipped { reason });
    }

    let pdf = generate_pdf(report);
    let encoded_pdf = STANDARD.encode(pdf);
    let html = format!(
        "<p>Your appointment report for {} is ready.</p>\
         <p><a download=\"appointment-report.pdf\" href=\"data:application/pdf;base64,{}\">Download PDF report</a></p>\
         <p>Contact your care team through its usual channel with clinical questions.</p>",
        escape_html(&report.occurred_at),
        encoded_pdf
    );

    let SendReceipt { message_id } = client
        .send(
            &report.patient_email,
            "Your appointment report",
            &html,
            &format!("appointment-report:{}", report.appointment_id),
        )
        .await?;

    Ok(DispatchOutcome::Sent { message_id })
}

fn generate_pdf(report: &AppointmentReport) -> Vec<u8> {
    let lines = [
        "Appointment report".to_owned(),
        format!("Appointment: {}", report.appointment_id),
        format!("Completed at: {}", report.occurred_at),
        "Status: completed".to_owned(),
    ];
    let text = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let escaped = line
                .replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)");
            format!("BT /F1 12 Tf 72 {} Td ({escaped}) Tj ET", 740 - index * 24)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(status: AppointmentStatus) -> AppointmentReport {
        AppointmentReport {
            appointment_id: "apt_2048".into(),
            patient_email: "patient@example.com".into(),
            occurred_at: "2026-09-18T09:30:00Z".into(),
            status,
        }
    }

    #[test]
    fn only_completed_appointments_are_dispatched() {
        assert_eq!(
            report(AppointmentStatus::Completed).dispatch_decision(),
            DispatchDecision::Send
        );
        assert!(matches!(
            report(AppointmentStatus::Scheduled).dispatch_decision(),
            DispatchDecision::Skip { .. }
        ));
        assert!(matches!(
            report(AppointmentStatus::Cancelled).dispatch_decision(),
            DispatchDecision::Skip { .. }
        ));
    }

    #[test]
    fn generated_report_is_a_pdf_without_patient_email() {
        let report = report(AppointmentStatus::Completed);
        let pdf = generate_pdf(&report);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(!String::from_utf8_lossy(&pdf).contains(&report.patient_email));
    }
}
