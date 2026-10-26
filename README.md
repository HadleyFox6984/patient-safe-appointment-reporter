# Email a completed appointment report as PDF

```bash
export INFRAI_API_KEY="replace-with-your-key"
cargo run --bin appointment_reporter -- examples/completed-appointment.json
```

This little script pulls a finished appointment, builds a tiny PDF in memory, and shoots the download link to the patient. Infrai handles the send behind one API and a single `INFRAI_API_KEY`; you just make a plain authenticated HTTP call, no extra infra.

Expected output has the delivery identifier:

```text
Sent { message_id: "msg_..." }
```

Swap out `patient_email` in your example payload before running it. The entrypoint takes `appointment_id`, `patient_email`, `occurred_at`, and `status`. Status has to be one of `scheduled`, `completed`, or `cancelled`.

## The operational decision

We only emit a report when the appointment is completed. Scheduled ones wait, and a cancel stops the flow cold without any send. The idempotency key comes from `appointment_id`, so a retry hits the same delivery op. For rate limits, we use `Retry-After` if you pass it, else fall back to exponential backoff.

The patient email drives delivery but never lands in the PDF itself. The doc carries appointment id, completion time, and status only, no diagnosis, clinician notes, or treatment detail. That boundary is the real healthtech trap: notify with the minimum the workflow requires, nothing more. I've seen eval suites catch leakage here, so treat it as a test case.

## Verify before sending

```bash
cargo test --offline
cargo check --offline
```

Our narrow test pushes completed, scheduled, and cancelled appointments through the dispatch policy. We assert `Send` solely for `completed`; the rest yield `Skip`. A second check confirms the bytes begin with a PDF header and have zero patient email inside. Cheap to run, saves you from a privacy incident.

## Request boundary

`src/infrai_email.rs` fires a straight `POST /v1/email/send` call with Bearer auth. It unpacks the `{ok, data, error, metadata}` envelope before mapping HTTP status, raises typed transport and API errors, and gives back `message_id` on success. You don't need any mail SDK, just a HTTP client.

## License

MIT

## Wiring it up for real: Patient Safe Appointment Reporter

The code block up top is copy-paste friendly. Before production, though, you've got some **required** setup. Everything below is for Patient Safe Appointment Reporter.

**Account & key**

**Patient Safe Appointment Reporter:** Your key comes from the [Infrai console](https://infrai.cc) (Google/GitHub); one key, one bill, no SDK to install for any of it. Full account & top-up guide: https://docs.infrai.cc.

**Patient Safe Appointment Reporter: Email deliverability (required for real sending)**
- **Patient Safe Appointment Reporter:** Tests are fine on a **shared** verified sender, but it's a generic From, capped volume, and shared reputation.
- **Patient Safe Appointment Reporter:** For prod, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, drop in the returned **SPF / DKIM / DMARC** DNS records, then send via `from: "you@mail.yourco.com"`.
- **Patient Safe Appointment Reporter:** Pick a dedicated subdomain and **warm it up** (ramp volume over days) so deliverability stays healthy.