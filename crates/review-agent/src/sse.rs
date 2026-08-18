use futures_util::{Stream, StreamExt};
use reqwest::Error as ReqwestError;

const MAX_EVENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SseError {
    #[error("stream read failed")]
    Read(#[source] ReqwestError),
    #[error("stream event exceeded the size limit")]
    TooLarge,
    #[error("stream contained invalid UTF-8")]
    InvalidUtf8,
    #[error("stream ended with an incomplete event")]
    Incomplete,
    #[error("stream protocol error: {0}")]
    Protocol(String),
}

pub(crate) async fn consume_sse<S, F>(stream: S, mut on_event: F) -> Result<(), SseError>
where
    S: Stream<Item = Result<bytes::Bytes, ReqwestError>>,
    F: FnMut(SseEvent) -> Result<bool, SseError>,
{
    futures_util::pin_mut!(stream);
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk.map_err(SseError::Read)?);
        if buffer.len() > MAX_EVENT_BYTES && find_event_boundary(&buffer).is_none() {
            return Err(SseError::TooLarge);
        }
        while let Some((end, delimiter_len)) = find_event_boundary(&buffer) {
            let frame = buffer[..end].to_vec();
            buffer.drain(..end + delimiter_len);
            if frame.len() > MAX_EVENT_BYTES {
                return Err(SseError::TooLarge);
            }
            if let Some(event) = parse_frame(&frame)? {
                if !on_event(event)? {
                    return Ok(());
                }
            }
        }
    }
    if buffer.iter().all(u8::is_ascii_whitespace) {
        Ok(())
    } else {
        Err(SseError::Incomplete)
    }
}

fn find_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4));
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2));
    match (crlf, lf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
}

fn parse_frame(frame: &[u8]) -> Result<Option<SseEvent>, SseError> {
    let text = std::str::from_utf8(frame).map_err(|_| SseError::InvalidUtf8)?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => event = Some(value.to_owned()),
            "data" => data.push(value),
            _ => {}
        }
    }
    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some(SseEvent {
            event,
            data: data.join("\n"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn chunks(parts: Vec<&[u8]>) -> impl Stream<Item = Result<bytes::Bytes, ReqwestError>> {
        let chunks = parts
            .into_iter()
            .map(|part| Ok(bytes::Bytes::copy_from_slice(part)))
            .collect::<Vec<_>>();
        stream::iter(chunks)
    }

    #[tokio::test]
    async fn parses_fragmented_utf8_crlf_comments_and_multiline_data() {
        let encoded = "你".as_bytes();
        let mut seen = Vec::new();
        consume_sse(
            chunks(vec![
                b": keep-alive\r\n\r\nevent: delta\r\ndata: hel",
                b"lo\r\ndata: ",
                &encoded[..1],
                &encoded[1..],
                b"\r\n\r\n",
            ]),
            |event| {
                seen.push(event);
                Ok(true)
            },
        )
        .await
        .unwrap();
        assert_eq!(
            seen,
            vec![SseEvent {
                event: Some("delta".into()),
                data: "hello\n你".into()
            }]
        );
    }

    #[tokio::test]
    async fn stops_when_the_consumer_is_done() {
        let mut count = 0;
        consume_sse(chunks(vec![b"data: one\n\ndata: two\n\n"]), |_| {
            count += 1;
            Ok(false)
        })
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn rejects_incomplete_and_oversized_events() {
        assert!(matches!(
            consume_sse(chunks(vec![b"data: partial"]), |_| Ok(true)).await,
            Err(SseError::Incomplete)
        ));
        let oversized = vec![b'x'; MAX_EVENT_BYTES + 1];
        assert!(matches!(
            consume_sse(
                stream::iter(vec![Ok(bytes::Bytes::from(oversized))]),
                |_| Ok(true)
            )
            .await,
            Err(SseError::TooLarge)
        ));
        assert!(matches!(
            consume_sse(chunks(vec![b"data: \xff\n\n"]), |_| Ok(true)).await,
            Err(SseError::InvalidUtf8)
        ));
    }
}
