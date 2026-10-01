//! Bounded classic single-member ZIP profile; unsupported containers fail closed.
//! ZIP byte fields are parsed only by official rc-zip public parsers.
use flate2::{Decompress, FlushDecompress, Status};
use rc_zip::{
    encoding::Encoding,
    parse::{
        CentralDirectoryFileHeader, DataDescriptorRecord, EndOfCentralDirectoryRecord, ExtraField,
        ExtraFieldSettings, LocalFileHeader, Method,
    },
};

use winnow::Partial;

const ARCHIVE_LIMIT: usize = 1024 * 1024;
const CSV_LIMIT: usize = 4 * 1024 * 1024;
type Result<T> = std::result::Result<T, &'static str>;
fn require(ok: bool, why: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(why)
    }
}

fn inflate(input: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = Decompress::new(false);
    let mut output = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let allowance = (CSV_LIMIT + 1 - output.len()).min(chunk.len());
        let status = decoder
            .decompress(
                &input[before_in as usize..],
                &mut chunk[..allowance],
                FlushDecompress::Finish,
            )
            .map_err(|_| "invalid deflate")?;
        let emitted = (decoder.total_out() - before_out) as usize;
        require(output.len() + emitted <= CSV_LIMIT, "decoded byte limit")?;
        output.extend_from_slice(&chunk[..emitted]);
        if status == Status::StreamEnd {
            require(
                decoder.total_in() == input.len() as u64,
                "unused compressed suffix",
            )?;
            return Ok(output);
        }
        require(
            decoder.total_in() != before_in || decoder.total_out() != before_out,
            "incomplete deflate",
        )?;
    }
}

pub(super) fn decode_zip(bytes: &[u8], expected: &[u8]) -> Result<Vec<u8>> {
    require(bytes.len() <= ARCHIVE_LIMIT, "archive byte limit")?;
    let window = bytes.len().saturating_sub(65 * 1024);
    let end = EndOfCentralDirectoryRecord::find_in_block(&bytes[window..]).ok_or("missing EOCD")?;
    let end_offset = window
        .checked_add(end.offset as usize)
        .ok_or("EOCD bounds")?;
    let mut end_input = Partial::new(&bytes[end_offset..]);
    let end = EndOfCentralDirectoryRecord::parser(&mut end_input).map_err(|_| "EOCD parse")?;
    require(end_input.is_empty(), "trailing archive bytes unsupported")?;
    require(
        end.disk_nbr == 0
            && end.dir_disk_nbr == 0
            && end.dir_records_this_disk == 1
            && end.directory_records == 1,
        "single disk and member required",
    )?;
    let start = end.directory_offset as usize;
    let central_end = start
        .checked_add(end.directory_size as usize)
        .ok_or("central range overflow")?;
    require(
        central_end == end_offset,
        "central range must end at classic EOCD",
    )?;
    let mut central_input = Partial::new(bytes.get(start..central_end).ok_or("central bounds")?);
    let central =
        CentralDirectoryFileHeader::parser(&mut central_input).map_err(|_| "central parse")?;
    require(
        central_input.is_empty(),
        "unconsumed central bytes or hidden member",
    )?;
    require(
        central.disk_nbr_start == 0 && central.header_offset == 0,
        "prefix or multi-disk unsupported",
    )?;
    require(
        central.reader_version.version <= 20,
        "required version unsupported",
    )?;
    require(central.name.as_ref() == expected, "raw central name")?;
    require(
        matches!(central.method, Method::Store | Method::Deflate),
        "compression unsupported",
    )?;
    require(central.flags & !0x080e == 0, "unsupported central flags")?;
    require(
        central.method == Method::Deflate || central.flags & 6 == 0,
        "store compression flags",
    )?;
    let mode = (central.external_attrs >> 16) & 0o170000;
    require(
        central.external_attrs & 0x10 == 0 && (mode == 0 || mode == 0o100000),
        "special file",
    )?;
    require(
        central.uncompressed_size as usize <= CSV_LIMIT,
        "declared decoded byte limit",
    )?;
    require(
        central.compressed_size as usize <= ARCHIVE_LIMIT,
        "declared compressed byte limit",
    )?;
    // The official parser validates extra fields. The classic profile cannot
    // admit extra fields that override the original central size or offset.
    classic_extra(
        &central.extra,
        ExtraFieldSettings {
            uncompressed_size_u32: central.uncompressed_size,
            compressed_size_u32: central.compressed_size,
            header_offset_u32: central.header_offset,
        },
    )?;
    let entry = central
        .as_entry(Encoding::Utf8, 0)
        .map_err(|_| "central extra fields")?;
    require(
        entry.uncompressed_size == central.uncompressed_size as u64
            && entry.compressed_size == central.compressed_size as u64
            && entry.header_offset == 0,
        "ZIP64 size or offset override unsupported",
    )?;
    let local_region = bytes.get(..start).ok_or("local bounds")?;
    let mut local_input = Partial::new(local_region);
    let local = LocalFileHeader::parser(&mut local_input).map_err(|_| "local parse")?;
    require(
        local.reader_version.version <= 20,
        "local required version unsupported",
    )?;
    require(
        local.name.as_ref() == expected
            && local.method == central.method
            && local.flags == central.flags,
        "local identity mismatch",
    )?;
    classic_extra(
        &local.extra,
        ExtraFieldSettings {
            uncompressed_size_u32: local.uncompressed_size,
            compressed_size_u32: local.compressed_size,
            header_offset_u32: 0,
        },
    )?;
    let local_entry = local.as_entry().map_err(|_| "local extra fields")?;
    require(
        local_entry.uncompressed_size == local.uncompressed_size as u64
            && local_entry.compressed_size == local.compressed_size as u64,
        "local ZIP64 override unsupported",
    )?;
    let data_offset = local_region.len() - local_input.len();
    let data_end = data_offset
        .checked_add(central.compressed_size as usize)
        .ok_or("data range overflow")?;
    require(data_end <= start, "data overlaps central directory")?;
    if central.flags & 8 != 0 {
        let mut descriptor = Partial::new(&bytes[data_end..start]);
        let record = DataDescriptorRecord::mk_parser(false)(&mut descriptor)
            .map_err(|_| "descriptor parse")?;
        require(
            descriptor.is_empty()
                && record.crc32 == central.crc32
                && record.compressed_size == central.compressed_size as u64
                && record.uncompressed_size == central.uncompressed_size as u64,
            "descriptor mismatch",
        )?;
    } else {
        require(
            data_end == start
                && local.crc32 == central.crc32
                && local.compressed_size == central.compressed_size
                && local.uncompressed_size == central.uncompressed_size,
            "local metadata or data extent mismatch",
        )?;
    }
    let compressed = &bytes[data_offset..data_end];
    let output = if central.method == Method::Store {
        compressed.to_vec()
    } else {
        inflate(compressed)?
    };
    require(
        output.len() == central.uncompressed_size as usize,
        "decoded size mismatch",
    )?;
    require(
        crc32fast::hash(&output) == central.crc32,
        "decoded CRC mismatch",
    )?;
    Ok(output)
}

// Use only the official typed extra-field parser, including exact remainder.
// Even a no-op ZIP64 extra is outside this deliberately classic profile.
fn classic_extra(bytes: &[u8], settings: ExtraFieldSettings) -> Result<()> {
    let mut input = Partial::new(bytes);
    while !input.is_empty() {
        let field =
            ExtraField::mk_parser(settings)(&mut input).map_err(|_| "invalid extra field")?;
        require(
            !matches!(field, ExtraField::Zip64(_) | ExtraField::Unknown { tag: 1 }),
            "ZIP64 extra unsupported",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    // Header mutations are confined to synthetic adversarial fixtures. Production
    // parsing above uses only the released library's public typed parsers.
    const FIXTURES: &str = r#"
import hashlib,io,json,pathlib,struct,sys,zipfile,zlib
root=pathlib.Path(sys.argv[1]); body=b'first\nsecond\n'; cases=[]
def save(name,data,accept,expected=body):
    (root/(name+'.zip')).write_bytes(data)
    cases.append({'name':name,'accept':accept,'expected_size':len(expected),'expected_sha256':hashlib.sha256(expected).hexdigest()})
class Streaming(io.BytesIO):
    def seekable(self): return False
    def seek(self,*args): raise io.UnsupportedOperation('streaming fixture')
def archive(method=8,stream=False,extra=b'',force64=False,body=body):
    out=Streaming() if stream else io.BytesIO()
    with zipfile.ZipFile(out,'w',compression=method) as z:
        z.comment=b'ordinary PK\x01\x02 PK\x06\x06 comment'
        info=zipfile.ZipInfo('selected.csv'); info.compress_type=method; info.extra=extra
        with z.open(info,'w',force_zip64=force64) as writer:writer.write(body)
    return out.getvalue()
for method in [0,8]:
    raw=archive(method); central=raw.index(b'PK\x01\x02'); end=raw.index(b'PK\x05\x06')
    save(f'normal_{method}',raw,True)
    save(f'descriptor_{method}',archive(method,stream=True),True)
    save(f'empty_{method}',archive(method,body=b''),True,b'')
    save(f'zip64_{method}',archive(method,force64=True),False)
    for label,visible in [('prefix',b'first\n'),('zero',b'')]:
        changed=bytearray(raw)
        for at in [14,central+16]:struct.pack_into('<I',changed,at,zlib.crc32(visible))
        for at in [22,central+24]:struct.pack_into('<I',changed,at,len(visible))
        save(f'hidden_{method}_{label}',changed,False)
    for label,at,value in [('local_version',4,99),('central_version',central+6,99),('encrypted',central+8,1),('local_flags',6,1),('central_method',central+10,12),('multidisk',end+4,1)]:
        changed=bytearray(raw);struct.pack_into('<H',changed,at,value);save(f'{label}_{method}',changed,False)
    changed=bytearray(raw);changed[30]=ord('x');save(f'local_name_{method}',changed,False)
    changed=bytearray(raw);struct.pack_into('<I',changed,central+20,len(raw));save(f'overlap_{method}',changed,False)
    duplicate=raw[:end]+raw[central:end]+raw[end:]
    dup=bytearray(duplicate);dup_end=end+(end-central)
    struct.pack_into('<I',dup,dup_end+12,2*(end-central));save(f'hidden_central_{method}',dup,False)
    save(f'trailer_{method}',raw+b'trailer',False)
    save(f'prefix_{method}',b'prefix'+raw,False)
save('unknown_extra',archive(extra=b'\xfe\xca\x03\x00xyz'),True)
save('malformed_extra',archive(extra=b'\xfe\xca\x03\x00x'),False)
save('noop_zip64_extra',archive(extra=b'\x01\x00\x00\x00'),False)
save('exact_decoded_limit',archive(body=b'a'*(4*1024*1024)),True,b'a'*(4*1024*1024))
save('over_decoded_limit',archive(body=b'a'*(4*1024*1024+1)),False)
bomb=bytearray(archive(body=b'a'*(64*1024*1024)));central=bomb.index(b'PK\x01\x02')
for at in [14,central+16,22,central+24]:struct.pack_into('<I',bomb,at,0)
save('false_zero_bomb',bomb,False)
(root/'cases.json').write_text(json.dumps(cases))
"#;

    #[test]
    fn typed_classic_zip_refuses_hidden_members_truncation_bombs_and_unsupported_containers() {
        let directory = tempfile::tempdir().unwrap();
        let result = Command::new("python3")
            .args(["-B", "-c", FIXTURES])
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let cases: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.path().join("cases.json")).unwrap())
                .unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let bytes = fs::read(directory.path().join(format!("{name}.zip"))).unwrap();
            let decoded = decode_zip(&bytes, b"selected.csv");
            assert_eq!(
                decoded.is_ok(),
                case["accept"].as_bool().unwrap(),
                "{name}: {decoded:?}"
            );
            if let Ok(bytes) = decoded {
                assert_eq!(
                    bytes.len() as u64,
                    case["expected_size"].as_u64().unwrap(),
                    "{name}"
                );
                assert_eq!(
                    super::super::bars::digest(&bytes),
                    case["expected_sha256"].as_str().unwrap(),
                    "{name}"
                );
            }
        }
    }
}
