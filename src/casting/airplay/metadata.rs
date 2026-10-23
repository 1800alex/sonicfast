//! Now-playing text, cover and progress for the receiver's own display, sent
//! as RTSP SET_PARAMETER bodies the way iTunes and OwnTone senders do.
use crate::casting::NowPlaying;
use std::io::Cursor;

/// Receivers show artwork small and some reject large images outright.
const ART_SIDE: u32 = 600;

/// DMAP `mlit` listing item: title (`minm`), artist (`asar`), album (`asal`).
/// Empty fields are still sent so a receiver clears the previous track's text.
pub fn dmap(now: &NowPlaying) -> Vec<u8> {
    let mut inner = Vec::new();
    for (tag, value) in [
        (b"minm", &now.title),
        (b"asar", &now.artist),
        (b"asal", &now.album),
    ] {
        inner.extend_from_slice(tag);
        inner.extend_from_slice(&(value.len() as u32).to_be_bytes());
        inner.extend_from_slice(value.as_bytes());
    }
    let mut out = b"mlit".to_vec();
    out.extend_from_slice(&(inner.len() as u32).to_be_bytes());
    out.extend(inner);
    out
}

/// `progress: start/current/end` in 44.1 kHz RTP frames, anchored so that
/// `current` is the frame the receiver is playing now.
pub fn progress(heard: u32, position_ms: u64, duration_ms: u64) -> String {
    let frames = |ms: u64| (ms.saturating_mul(44_100) / 1000) as u32;
    let start = heard.wrapping_sub(frames(position_ms));
    format!(
        "progress: {start}/{heard}/{}\r\n",
        start.wrapping_add(frames(duration_ms))
    )
}

/// Cover art re-encoded as a JPEG no larger than 600 px a side, dropping
/// any transparency; None when the bytes are not a decodable image.
pub fn artwork_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = if image.width() > ART_SIDE || image.height() > ART_SIDE {
        image.thumbnail(ART_SIDE, ART_SIDE)
    } else {
        image
    };
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80)
        .encode_image(&image.into_rgb8())
        .ok()?;
    Some(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmap_wraps_title_artist_album_in_a_listing_item() {
        let body = dmap(&NowPlaying {
            title: "Song".into(),
            artist: "Bänd".into(),
            album: String::new(),
            art_url: None,
        });
        let mut expected = b"mlit".to_vec();
        expected.extend_from_slice(&(12u32 + 13 + 8).to_be_bytes());
        expected.extend_from_slice(b"minm\0\0\0\x04Song");
        expected.extend_from_slice(b"asar\0\0\0\x05B\xc3\xa4nd");
        expected.extend_from_slice(b"asal\0\0\0\0");
        assert_eq!(body, expected);
    }

    #[test]
    fn progress_anchors_the_heard_frame_at_the_track_position() {
        assert_eq!(
            progress(1_000_000, 2_000, 10_000),
            "progress: 911800/1000000/1352800\r\n"
        );
        // RTP timestamps start at a random value and wrap.
        assert_eq!(
            progress(100, 1_000, 2_000),
            format!(
                "progress: {}/100/{}\r\n",
                100u32.wrapping_sub(44_100),
                100u32.wrapping_sub(44_100).wrapping_add(88_200)
            )
        );
    }

    #[test]
    fn artwork_is_scaled_to_a_small_jpeg_and_garbage_is_skipped() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(1200, 800, image::Rgba([200, 30, 30, 128]))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg = artwork_jpeg(&png).unwrap();
        assert_eq!(
            image::guess_format(&jpeg).unwrap(),
            image::ImageFormat::Jpeg
        );
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (600, 400));
        assert!(artwork_jpeg(b"not an image").is_none());
    }
}
