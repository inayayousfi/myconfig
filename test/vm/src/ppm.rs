//! Reads the binary PPM screenshots that QEMU's `screendump` writes, to tell a drawn screen
//! from a blank one.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PpmError {
    #[error("the screenshot is not a binary PPM file")]
    NotPpm,
    #[error("the PPM header is not valid")]
    Header,
    #[error("the PPM file holds {found} bytes of pixels, not the {expected} its header names")]
    Truncated { expected: usize, found: usize },
}

/// Whether every pixel of the binary PPM image `bytes` has the same color. A screen of one
/// color shows nothing: the program that should draw on it has not drawn.
pub fn is_single_color(bytes: &[u8]) -> Result<bool, PpmError> {
    let (width, height, sample_size, pixels) = parse(bytes)?;
    let pixel_size = 3 * sample_size;
    let expected = width * height * pixel_size;
    if pixels.len() < expected {
        return Err(PpmError::Truncated {
            expected,
            found: pixels.len(),
        });
    }
    let mut chunks = pixels[..expected].chunks_exact(pixel_size);
    let Some(first) = chunks.next() else {
        return Ok(true);
    };
    Ok(chunks.all(|pixel| pixel == first))
}

/// The width, height, bytes per sample and pixel bytes of a binary PPM image.
fn parse(bytes: &[u8]) -> Result<(usize, usize, usize, &[u8]), PpmError> {
    let rest = bytes.strip_prefix(b"P6").ok_or(PpmError::NotPpm)?;
    let mut position = 0;
    let mut numbers = [0usize; 3];
    for number in &mut numbers {
        position = skip_space_and_comments(rest, position);
        let start = position;
        while rest.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
        *number = std::str::from_utf8(&rest[start..position])
            .ok()
            .and_then(|digits| digits.parse().ok())
            .ok_or(PpmError::Header)?;
    }
    // Exactly one whitespace byte separates the header from the pixels.
    if !rest.get(position).is_some_and(u8::is_ascii_whitespace) {
        return Err(PpmError::Header);
    }
    let [width, height, maximum] = numbers;
    let sample_size = match maximum {
        1..=255 => 1,
        256..=65535 => 2,
        _ => return Err(PpmError::Header),
    };
    Ok((width, height, sample_size, &rest[position + 1..]))
}

fn skip_space_and_comments(bytes: &[u8], mut position: usize) -> usize {
    loop {
        match bytes.get(position) {
            Some(byte) if byte.is_ascii_whitespace() => position += 1,
            Some(b'#') => {
                while bytes.get(position).is_some_and(|&byte| byte != b'\n') {
                    position += 1;
                }
            }
            _ => return position,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: usize, height: usize, pixels: &[[u8; 3]]) -> Vec<u8> {
        let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
        for pixel in pixels {
            bytes.extend_from_slice(pixel);
        }
        bytes
    }

    #[test]
    fn a_screen_of_one_color_is_blank() {
        let black = image(2, 2, &[[0, 0, 0]; 4]);
        assert_eq!(is_single_color(&black), Ok(true));
    }

    #[test]
    fn one_pixel_of_another_color_makes_a_drawn_screen() {
        let pink = [255, 78, 173];
        let drawn = image(2, 2, &[[0, 0, 0], [0, 0, 0], [0, 0, 0], pink]);
        assert_eq!(is_single_color(&drawn), Ok(false));
    }

    #[test]
    fn a_comment_in_the_header_is_skipped() {
        let mut bytes = b"P6\n# written by QEMU\n2 1\n255\n".to_vec();
        bytes.extend_from_slice(&[1, 2, 3, 1, 2, 4]);
        assert_eq!(is_single_color(&bytes), Ok(false));
    }

    #[test]
    fn a_png_or_a_cut_file_is_an_error_not_a_blank_screen() {
        assert_eq!(is_single_color(b"\x89PNG\r\n"), Err(PpmError::NotPpm));
        let cut = image(2, 2, &[[0, 0, 0]; 3]);
        assert_eq!(
            is_single_color(&cut),
            Err(PpmError::Truncated {
                expected: 12,
                found: 9
            })
        );
    }
}
