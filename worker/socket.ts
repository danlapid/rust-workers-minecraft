export async function forwardSocket(socket: Socket, target: Socket, readable = socket.readable): Promise<void> {
  // Read/write failures are observed by the pumps below. Also consume the
  // lifetime promises so a normal peer disconnect is not an unhandled rejection.
  void socket.closed.catch(() => undefined);
  void target.closed.catch(() => undefined);
  const abort = new AbortController();
  const pipe = (source: ReadableStream, destination: WritableStream) =>
    source.pipeTo(destination, { signal: abort.signal }).catch(error => {
      abort.abort();
      throw error;
    });
  try {
    const results = await Promise.allSettled([
      pipe(readable, target.writable),
      pipe(target.readable, socket.writable),
    ]);
    for (const result of results) {
      if (result.status === 'rejected' && !/closed|closing|abort|cancel|reset|network connection lost/i.test(String(result.reason))) {
        console.error('TCP forwarding failed', result.reason);
      }
    }
  } finally {
    await Promise.allSettled([socket.close(), target.close()]);
  }
}
