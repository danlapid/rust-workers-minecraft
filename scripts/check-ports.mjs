import net from 'node:net';

for (const value of process.argv.slice(2)) {
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error(`Invalid port: ${value}`);
  const listener = net.createServer();
  try {
    await new Promise((resolve, reject) => {
      listener.once('error', reject);
      listener.listen(port, '127.0.0.1', resolve);
    });
    await new Promise(resolve => listener.close(resolve));
  } catch (error) {
    if (error.code !== 'EADDRINUSE') throw error;
    console.error(`error: localhost:${port} is occupied. Stop the existing server before starting another.`);
    process.exit(1);
  }
}
