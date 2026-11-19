//! Reads the binary PPM screenshots that QEMU's `screendump` writes, to tell a drawn screen
//! from a blank one and to find where two screens differ.

use std::ops::Range;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PpmError {
    #[error("the screenshot is not a binary PPM file")]
    NotPpm,
    #[error("the PPM header is not valid")]
    Header,
    #[error("the PPM file holds {found} bytes of pixels, not the {expected} its header names")]
    Truncated { expected: usize, found: usize },
    #[error("the two screenshots differ in size or color depth")]
    Sizes,
}

/// Whether every pixel of the binary PPM image `bytes` has the same color. A screen of one
/// color shows nothing: the program that should draw on it has not drawn.
pub fn is_single_color(bytes: &[u8]) -> Result<bool, PpmError> {
    let (_, _, sample_size, pixels) = pixels(bytes)?;
    let mut chunks = pixels.chunks_exact(3 * sample_size);
    let Some(first) = chunks.next() else {
        return Ok(true);
    };
    Ok(chunks.all(|pixel| pixel == first))
}

/// The width and height of the binary PPM image `bytes`.
pub fn size(bytes: &[u8]) -> Result<(usize, usize), PpmError> {
    let (width, height, _, _) = parse(bytes)?;
    Ok((width, height))
}

/// A rectangle of pixels, both ends of each range included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub left: usize,
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
}

impl Area {
    pub fn width(self) -> usize {
        self.right - self.left + 1
    }

    pub fn height(self) -> usize {
        self.bottom - self.top + 1
    }
}

/// The smallest rectangle that holds every pixel of the rows `rows` whose color differs
/// between the screens `before` and `after`, or none when those rows are the same. The two
/// screens must have the same size.
pub fn changed_area(
    before: &[u8],
    after: &[u8],
    rows: Range<usize>,
) -> Result<Option<Area>, PpmError> {
    let (width, height, sample_size, first) = pixels(before)?;
    let (other_width, other_height, other_sample_size, second) = pixels(after)?;
    if (width, height, sample_size) != (other_width, other_height, other_sample_size) {
        return Err(PpmError::Sizes);
    }
    let pixel_size = 3 * sample_size;
    let mut area: Option<Area> = None;
    for y in rows.start.min(height)..rows.end.min(height) {
        for x in 0..width {
            let at = (y * width + x) * pixel_size;
            if first[at..at + pixel_size] == second[at..at + pixel_size] {
                continue;
            }
            area = Some(match area {
                None => Area {
                    left: x,
                    top: y,
                    right: x,
                    bottom: y,
                },
                Some(area) => Area {
                    left: area.left.min(x),
                    top: area.top.min(y),
                    right: area.right.max(x),
                    bottom: area.bottom.max(y),
                },
            });
        }
    }
    Ok(area)
}

/// The width, height, bytes per sample and the pixel bytes of a complete binary PPM image.
fn pixels(bytes: &[u8]) -> Result<(usize, usize, usize, &[u8]), PpmError> {
    let (width, height, sample_size, pixels) = parse(bytes)?;
    let expected = width * height * 3 * sample_size;
    if pixels.len() < expected {
        return Err(PpmError::Truncated {
            expected,
            found: pixels.len(),
        });
    }
    Ok((width, height, sample_size, &pixels[..expected]))
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
    fn the_changed_area_covers_the_differing_pixels_of_the_chosen_rows_only() {
        let black = [0, 0, 0];
        let pink = [255, 78, 173];
        let before = image(4, 3, &[black; 12]);
        let mut pixels = [black; 12];
        // Row 0 changes at x 3, row 1 at x 1 and 2, row 2 stays.
        pixels[3] = pink;
        pixels[5] = pink;
        pixels[6] = pink;
        let after = image(4, 3, &pixels);
        assert_eq!(
            changed_area(&before, &after, 1..3),
            Ok(Some(Area {
                left: 1,
                top: 1,
                right: 2,
                bottom: 1
            }))
        );
        assert_eq!(
            changed_area(&before, &after, 0..3)
                .unwrap()
                .map(Area::width),
            Some(3)
        );
        assert_eq!(changed_area(&before, &before, 0..3), Ok(None));
        assert_eq!(
            changed_area(&before, &image(2, 2, &[black; 4]), 0..2),
            Err(PpmError::Sizes)
        );
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
