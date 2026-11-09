use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    time::{Duration, Instant},
};

fn ply() -> (Vec<u8>, Vec<u8>) {
    let mut head = b"ply\nformat binary_little_endian 1.0\nelement vertex 2\n".to_vec();
    for key in [
        "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "scale_0", "scale_1", "scale_2", "opacity",
        "rot_0", "rot_1", "rot_2", "rot_3",
    ] {
        head.extend_from_slice(format!("property float {key}\n").as_bytes());
    }
    head.extend_from_slice(b"end_header\n");
    let row = [
        0_f32, 0., -3., 1., 0., 0., -2., -2., -2., 0., 1., 0., 0., 0.,
    ]
    .into_iter()
    .flat_map(f32::to_le_bytes)
    .collect();
    (head, row)
}
fn next(download: &Download) -> Event {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = download.pop() {
            return event;
        }
        assert!(
            Instant::now() < until,
            "timed out awaiting progressive delivery"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn slow_http_delivers_a_batch_before_the_server_finishes() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let (release, wait) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = server.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = [0; 2048];
        assert!(socket.read(&mut request).unwrap() > 0);
        let (header, row) = ply();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            header.len() + row.len() * 2
        )
        .unwrap();
        socket.write_all(&header).unwrap();
        socket.write_all(&row).unwrap();
        socket.flush().unwrap();
        wait.recv_timeout(Duration::from_secs(10)).unwrap();
        socket.write_all(&row).unwrap();
    });
    let download = Download::start(format!("http://{address}/scene.ply"), None);
    let Event::Batch(first) = next(&download) else {
        panic!("first event must be a decoded batch")
    };
    assert_eq!(first.count, 1);
    assert_eq!(first.total, 2);
    assert!(download.pop().is_none());
    release.send(()).unwrap();
    let Event::Batch(second) = next(&download) else {
        panic!("second row missing")
    };
    assert_eq!(second.count, 1);
    assert!(matches!(next(&download), Event::Complete));
    worker.join().unwrap();
}
#[test]
fn producer_queue_is_bounded_and_dropping_download_cancels_it() {
    let (download, producer) = Download::channel(None);
    assert!(producer.try_send(Event::Complete).is_none());
    assert!(producer.try_send(Event::Complete).is_none());
    assert!(producer.try_send(Event::Complete).is_some());
    assert!(download.pop().is_some());
    assert!(producer.try_send(Event::Complete).is_none());
    drop(download);
    assert!(!producer.alive());
    assert!(producer.try_send(Event::Complete).is_some());
}
#[test]
fn unsupported_encoding_falls_back_with_original_bytes_and_truncation_fails() {
    let (download, mut producer) = Download::channel(None);
    let ascii = include_bytes!("../../tests/fixtures/tiny.ply");
    let events = producer.decode(ascii).unwrap();
    assert!(events.is_empty());
    let Event::Fallback(bytes) = producer.finish().unwrap() else {
        panic!("ASCII must use whole-file fallback")
    };
    assert_eq!(bytes, ascii);
    drop(download);
    let (_download, mut producer) = Download::channel(None);
    let (mut header, row) = ply();
    header.extend_from_slice(&row);
    producer.decode(&header).unwrap();
    assert!(producer.finish().is_err());
}
