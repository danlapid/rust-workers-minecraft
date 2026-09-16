// Emscripten library adjustments for NODERAWFS on workerd's Node filesystem.
addToLibrary({
  // Node's deprecated process.binding is absent; fs.constants is the public
  // form of the same table (emscripten-core/emscripten cf-final 4e034c65a).
  $NODEFS__postset: `
    NODEFS.isWindows = false;
    NODEFS.flagsForNodeMap = Object.fromEntries([
      [{{{ cDefs.O_APPEND }}}, 'O_APPEND'],
      [{{{ cDefs.O_CREAT }}}, 'O_CREAT'],
      [{{{ cDefs.O_EXCL }}}, 'O_EXCL'],
      [{{{ cDefs.O_NOCTTY }}}, 'O_NOCTTY'],
      [{{{ cDefs.O_RDONLY }}}, 'O_RDONLY'],
      [{{{ cDefs.O_RDWR }}}, 'O_RDWR'],
      [{{{ cDefs.O_DSYNC }}}, 'O_SYNC'],
      [{{{ cDefs.O_TRUNC }}}, 'O_TRUNC'],
      [{{{ cDefs.O_WRONLY }}}, 'O_WRONLY'],
      [{{{ cDefs.O_NOFOLLOW }}}, 'O_NOFOLLOW'],
    ].map(([flag, name]) => [flag, fs.constants[name]]));
  `,

  // workerd has no process stdio descriptors: keep fds 0-2 on Emscripten's
  // console callbacks so Pumpkin's log and Rust panics reach the host console.
  // Under NODERAWFS the host enforces permissions; workerd reports directories
  // as 0666, which would otherwise fail the VFS's execute check in getdents.
  $workerdStdio__deps: ['$FS', '$TTY'],
  $workerdStdio__postset: () => addAtPreRun('workerdStdio()'),
  $workerdStdio: () => {
    Object.defineProperty(FS, 'ignorePermissions', { get: () => true, set() {} });
    const consoles = [
      null,
      { output: [], ops: TTY.default_tty_ops },
      { output: [], ops: TTY.default_tty1_ops },
    ];
    const fstat = FS.fstat;
    FS.fstat = (fd) => {
      const stream = FS.getStreamChecked(fd);
      if (stream.nfd === 0 || stream.nfd === 1 || stream.nfd === 2) {
        return {
          dev: 0, ino: stream.nfd, mode: 0o020666, nlink: 1, uid: 0, gid: 0, rdev: 0,
          size: 0, blksize: 4096, blocks: 0,
          atime: new Date(0), mtime: new Date(0), ctime: new Date(0),
        };
      }
      return fstat(fd);
    };
    const write = FS.write;
    FS.write = (stream, buffer, offset, length, position) => {
      const tty = consoles[stream.nfd];
      if (!tty) return write(stream, buffer, offset, length, position);
      for (let i = offset; i < offset + length; i++) tty.ops.put_char(tty, buffer[i]);
      return length;
    };
  },
});
