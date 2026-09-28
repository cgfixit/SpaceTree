use spacetree::{ext_color, ext_description, ext_key, ext_label, format_scan_share, share_px, Rgb};

#[test]
fn extension_key_label_and_known_color() {
    assert_eq!(
        ext_color(&ext_key("landscape-01.JPG")),
        Rgb {
            r: 220,
            g: 60,
            b: 160
        }
    );
    assert_eq!(ext_label(&ext_key("archive.tar.gz")), ".gz");
    assert_eq!(ext_label(&ext_key("Makefile")), "(none)");
    assert_eq!(ext_description(""), "No Extension");
    assert_eq!(ext_description("mp4"), "MP4 Video");
    assert_eq!(ext_description("zzz"), "ZZZ File");
}

#[test]
fn scan_share_uses_the_root_not_a_stored_percent() {
    assert_eq!(format_scan_share(0, 0), "0.00%");
    assert_eq!(format_scan_share(5, 5), "100.00%");
    assert_eq!(format_scan_share(1, 4), "25.00%");
    assert_eq!(share_px(1, 4, 100), 25);
    assert_eq!(share_px(1, 4, 0), 0);
    assert_eq!(share_px(1, 0, 100), 0);
}
