//! Pair-to-Phone QR code generator (M204).
//!
//! Detects the local network IP, builds a Web UI URL, and renders it as
//! a QR code image that phones can scan to access irontide remotely.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

const QR_SCALE: u32 = 8;
const QR_QUIET_ZONE: u32 = 2;

#[must_use]
pub fn detect_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_loopback() || ip == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
        return None;
    }
    Some(ip)
}

pub struct PhonePairInfo {
    pub url: String,
    pub local_ip: String,
    pub port: u16,
    pub qr_image: slint::Image,
}

#[must_use]
pub fn generate_pair_info(port: u16) -> PhonePairInfo {
    let ip = detect_local_ip().map_or_else(|| "127.0.0.1".to_string(), |a| a.to_string());

    let url = format!("http://{ip}:{port}/webui/");

    let qr_image = render_qr_image(&url);

    PhonePairInfo {
        url,
        local_ip: ip,
        port,
        qr_image,
    }
}

#[must_use]
fn render_qr_image(data: &str) -> slint::Image {
    let Ok(code) = qrcode::QrCode::new(data.as_bytes()) else {
        return blank_image();
    };

    let modules = code.to_colors();
    #[allow(clippy::cast_possible_truncation, reason = "QR width is at most 177")]
    let qr_width = code.width() as u32;
    let img_size = (qr_width + QR_QUIET_ZONE * 2) * QR_SCALE;

    let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(img_size, img_size);
    let pixels = buf.make_mut_bytes();

    for byte in pixels.iter_mut() {
        *byte = 255;
    }

    for row in 0..qr_width {
        for col in 0..qr_width {
            let is_dark = modules[(row * qr_width + col) as usize] == qrcode::Color::Dark;
            if !is_dark {
                continue;
            }
            let px_row_start = (row + QR_QUIET_ZONE) * QR_SCALE;
            let px_col_start = (col + QR_QUIET_ZONE) * QR_SCALE;
            for dy in 0..QR_SCALE {
                for dx in 0..QR_SCALE {
                    let pr = px_row_start + dy;
                    let pc = px_col_start + dx;
                    let idx = ((pr * img_size + pc) * 3) as usize;
                    pixels[idx] = 0;
                    pixels[idx + 1] = 0;
                    pixels[idx + 2] = 0;
                }
            }
        }
    }

    slint::Image::from_rgb8(buf)
}

#[must_use]
fn blank_image() -> slint::Image {
    let buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(1, 1);
    slint::Image::from_rgb8(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_local_ip_returns_non_loopback() {
        if let Some(ip) = detect_local_ip() {
            assert!(!ip.is_loopback());
            assert!(!ip.is_unspecified());
        }
    }

    #[test]
    fn generate_pair_info_produces_valid_url() {
        let info = generate_pair_info(9080);
        assert!(info.url.starts_with("http://"));
        assert!(info.url.contains(":9080/webui/"));
        assert_eq!(info.port, 9080);
        assert!(!info.local_ip.is_empty());
    }

    #[test]
    fn generate_pair_info_custom_port() {
        let info = generate_pair_info(8080);
        assert!(info.url.contains(":8080/webui/"));
        assert_eq!(info.port, 8080);
    }

    #[test]
    fn render_qr_image_produces_nonzero_size() {
        let img = render_qr_image("http://192.168.1.100:9080/webui/");
        let size = img.size();
        assert!(size.width > 0);
        assert!(size.height > 0);
        assert_eq!(size.width, size.height);
    }

    #[test]
    fn render_qr_image_empty_input() {
        let img = render_qr_image("");
        let size = img.size();
        assert!(size.width > 0);
    }

    #[test]
    fn render_qr_image_long_url() {
        let long = format!("http://192.168.1.100:9080/webui/?token={}", "a".repeat(200));
        let img = render_qr_image(&long);
        let size = img.size();
        assert!(size.width > 0);
        assert_eq!(size.width, size.height);
    }

    #[test]
    fn blank_image_is_1x1() {
        let img = blank_image();
        assert_eq!(img.size().width, 1);
        assert_eq!(img.size().height, 1);
    }

    #[test]
    fn qr_scale_produces_expected_dimensions() {
        let code = qrcode::QrCode::new(b"test").unwrap();
        #[allow(clippy::cast_possible_truncation, reason = "QR width is at most 177")]
        let qr_w = code.width() as u32;
        let expected = (qr_w + QR_QUIET_ZONE * 2) * QR_SCALE;
        let img = render_qr_image("test");
        assert_eq!(img.size().width, expected);
    }
}
