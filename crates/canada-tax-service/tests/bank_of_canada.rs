//! The rate source against a local stand-in that serves a real answer of the
//! Bank of Canada (saved 2026-11-12). The live API is not called by tests.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use chrono::NaiveDate;
use loontax_canada_tax::{Error, RateSource};
use loontax_canada_tax_service::BankOfCanada;
use rust_decimal_macros::dec;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const ANSWER: &str = include_str!("valet-answer.json");

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

/// Serves one request with `status` and `body`, and returns the request line.
async fn serve_once(
    status: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/valet", listener.local_addr().unwrap());
    let served = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.ok();
        String::from_utf8_lossy(&request)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    });
    (base, served)
}

#[tokio::test]
async fn fetches_and_reads_the_rates() {
    let (base, served) = serve_once("200 OK", ANSWER).await;
    let bank = BankOfCanada::with_base_url(&base).unwrap();

    let rates = bank.fetch("USD", day(6, 29), day(7, 3)).await.unwrap();

    assert_eq!(rates.len(), 4);
    assert_eq!(rates[1], (day(6, 30), dec!(1.4210)));
    // Only the series and the two dates are sent.
    assert_eq!(
        served.await.unwrap(),
        "GET /valet/observations/FXUSDCAD/json?start_date=2026-06-29&end_date=2026-07-03 HTTP/1.1"
    );
}

#[tokio::test]
async fn an_error_status_is_a_fetch_failure() {
    let (base, _served) = serve_once(
        "404 Not Found",
        r#"{"message":"Series FXXXXCAD not found."}"#,
    )
    .await;
    let bank = BankOfCanada::with_base_url(&base).unwrap();
    assert_eq!(
        bank.fetch("USD", day(6, 29), day(7, 3)).await,
        Err(Error::RateFetch(
            "the Bank of Canada answered with status 404".to_string()
        ))
    );
}

#[tokio::test]
async fn an_unreachable_bank_is_a_fetch_failure() {
    // A port nothing listens on.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/valet", listener.local_addr().unwrap());
    drop(listener);
    let bank = BankOfCanada::with_base_url(&base).unwrap();
    assert!(matches!(
        bank.fetch("USD", day(6, 29), day(7, 3)).await,
        Err(Error::RateFetch(_))
    ));
}
