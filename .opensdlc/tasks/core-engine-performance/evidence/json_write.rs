use std::fs::File;
use std::io::{Write, BufWriter};
use std::time::Instant;
struct LimitedFile { file: File, remaining: u64, written: u64, calls: u64 }
impl Write for LimitedFile {
 fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
  if bytes.len() as u64 > self.remaining { return Err(std::io::Error::other("NATIVE_OUTPUT_LIMIT")); }
  self.calls += 1;
  let written = self.file.write(bytes)?;
  self.remaining -= written as u64;
  self.written += written as u64;
  Ok(written)
 }
 fn flush(&mut self) -> std::io::Result<()> { self.file.flush() }
}
fn main() {
 let mode = std::env::args().nth(1).unwrap();
 let path = std::env::args().nth(2).unwrap();
 let rows: Vec<_> = (0..10000).map(|i| serde_json::json!({"instrument_id":"AAPL.XNAS","ts_event":"1785381300000000000","expected_return":0.000123_f64+i as f64*0.00000001,"uncertainty":0.00045,"index":i})).collect();
 let mut stream=LimitedFile { file:File::create(&path).unwrap(),remaining:64*1024*1024,written:0,calls:0};
 let start=Instant::now();
 if mode == "buffered" { let mut b=BufWriter::with_capacity(65536,&mut stream); serde_json::to_writer(&mut b,&rows).unwrap(); b.flush().unwrap(); }
 else { serde_json::to_writer(&mut stream,&rows).unwrap(); }
 stream.flush().unwrap(); stream.file.sync_all().unwrap();
 eprintln!("mode={} bytes={} calls={} write_sync_ms={:.3}", mode,stream.written,stream.calls,start.elapsed().as_secs_f64()*1000.0);
}
