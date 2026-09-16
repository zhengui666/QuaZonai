// Native image assembly only. File descriptors, not check-then-read paths, own
// the selected bytes. Existing destinations are never overwritten or followed.
import fs from 'node:fs';
import path from 'node:path';

const CHUNK = 1024 * 1024;

function sameObservation(before, after) {
  return before.dev === after.dev && before.ino === after.ino && before.size === after.size
    && before.mtimeNs === after.mtimeNs && before.ctimeNs === after.ctimeNs;
}

export function nativeDestination(root, target) {
  if (!path.isAbsolute(root) || !path.isAbsolute(target) || path.normalize(target) !== target || target === '/') {
    throw new Error('Invalid native image path');
  }
  return path.join(root, target.slice(1));
}

export function copyNativeFile(root, source, target) {
  const destination = nativeDestination(root, target);
  // Native toolchains legitimately contain source symlinks. Select their actual
  // file once, then perform every observation/read on that open descriptor.
  const sourceFd = fs.openSync(source, fs.constants.O_RDONLY | fs.constants.O_NONBLOCK);
  let destinationFd;
  let created = false;
  try {
    const original = fs.fstatSync(sourceFd, { bigint: true });
    if (!original.isFile() || original.size > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error('Native library is not a bounded regular file');
    }
    fs.mkdirSync(path.dirname(destination), { recursive: true, mode: 0o755 });
    try {
      destinationFd = fs.openSync(destination,
        fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_NOFOLLOW,
        0o600);
      created = true;
    } catch (error) {
      if (error?.code !== 'EEXIST') throw error;
      destinationFd = fs.openSync(destination,
        fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
    }
    const previous = fs.fstatSync(destinationFd, { bigint: true });
    if (!previous.isFile() || (!created && previous.size !== original.size)) {
      throw new Error('Conflicting native library');
    }
    const incoming = Buffer.allocUnsafe(CHUNK);
    const existing = created ? null : Buffer.allocUnsafe(CHUNK);
    const length = Number(original.size);
    for (let position = 0; position < length;) {
      const requested = Math.min(CHUNK, length - position);
      const count = fs.readSync(sourceFd, incoming, 0, requested, position);
      if (count === 0) throw new Error('Native source changed during image assembly');
      if (created) {
        let written = 0;
        while (written < count) {
          const step = fs.writeSync(destinationFd, incoming, written, count - written, position + written);
          if (step === 0) throw new Error('Native image copy made no progress');
          written += step;
        }
      } else {
        let read = 0;
        while (read < count) {
          const step = fs.readSync(destinationFd, existing, read, count - read, position + read);
          if (step === 0) throw new Error('Conflicting native library');
          read += step;
        }
        if (!incoming.subarray(0, count).equals(existing.subarray(0, count))) {
          throw new Error('Conflicting native library');
        }
      }
      position += count;
    }
    if (!sameObservation(original, fs.fstatSync(sourceFd, { bigint: true }))) {
      throw new Error('Native source changed during image assembly');
    }
    if (created) {
      fs.fchmodSync(destinationFd, Number(original.mode & 0o777n));
      fs.fsyncSync(destinationFd);
    } else if (!sameObservation(previous, fs.fstatSync(destinationFd, { bigint: true }))) {
      throw new Error('Native destination changed during image assembly');
    }
  } finally {
    if (destinationFd !== undefined) fs.closeSync(destinationFd);
    fs.closeSync(sourceFd);
  }
}
