import type { FastifyReply } from 'fastify';
import type { WatchHandle } from '@earendil-works/chord';

/** An HTTP subscription owns only its observers; losing it never aborts durable work. */
export async function eventStream(reply: FastifyReply, event: string, read: () => Promise<unknown>,
  watches: Pick<WatchHandle<unknown>, 'start' | 'stop' | 'closed'>[], streams: Set<() => Promise<void>>) {
  let closed = false;
  let tail = Promise.resolve();
  let queued = false;
  reply.hijack();
  reply.raw.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache, no-transform', connection: 'keep-alive', 'x-accel-buffering': 'no' });
  async function close() {
    if (closed) return;
    closed = true; streams.delete(close); reply.raw.end();
    await Promise.allSettled(watches.map(watch => watch.stop()));
  }
  async function publish() {
    if (closed) return;
    const value = await read();
    if (closed) return;
    if (!reply.raw.write(`event: ${event}\ndata: ${JSON.stringify(value)}\n\n`)) await new Promise<void>(resolve => {
      const done = () => { reply.raw.off('drain', done); reply.raw.off('close', done); resolve(); };
      reply.raw.once('drain', done); reply.raw.once('close', done);
    });
  }
  function changed() {
    // These are invalidations of a complete latest-state projection, not event
    // records. Several watches often invalidate the same projection together.
    // Keep one pending read; an invalidation during read/drain still queues the
    // next read so it cannot disappear behind an older in-flight snapshot.
    if (closed || queued) return tail;
    queued = true;
    tail = tail.then(async () => { queued = false; await publish(); })
      .catch(() => { if (!closed) reply.raw.end(); });
    return tail;
  }
  streams.add(close);
  reply.raw.once('close', () => { void close(); });
  for (const watch of watches) void watch.closed.then(close, close);
  for (const watch of watches) watch.start(async call => { if (call.kind === 'value') await changed(); });
  await changed();
}
