// Emscripten platform hooks for Workers.
addToLibrary({
  $workerdHostTasks__deps: ['$emSetImmediate', '$emClearImmediate'],
  $workerdHostTasks__postset: () => addAtPreRun('workerdHostTasks()'),
  $workerdHostTasks: () => {
    const pending = new Map();
    let nextId = 0;
    // Workers implements setImmediate as a zero-delay timer. Its callback
    // caps the clock at that timer's deadline, including nested microtasks.
    // scheduler.wait resumes outside the callback, after the cap is released.
    emSetImmediate = (callback) => {
      do { nextId = (nextId + 1) & 0x7fffffff; } while (!nextId || pending.has(nextId));
      const id = nextId;
      const controller = new AbortController();
      pending.set(id, controller);
      void scheduler.wait(0, { signal: controller.signal }).then(() => {
        // Cancellation can race with an already-fulfilled wait promise.
        if (pending.delete(id)) callback();
      }).catch(error => {
        pending.delete(id);
        if (!controller.signal.aborted) queueMicrotask(() => { throw error; });
      });
      return id;
    };
    emClearImmediate = (id) => {
      const controller = pending.get(id);
      if (!controller) return false;
      pending.delete(id);
      controller.abort();
      return true;
    };
  },

  // Node's deprecated process.binding is absent; fs.constants is the public
  // form of the same table.
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

  // The host enforces permissions; workerd reports directories as 0666, which
  // would otherwise fail the VFS's execute check in getdents. Once the object
  // mounts its storage (src/js/mount.js), NODERAWFS reads and writes through
  // worker-fs-mount's fs, which routes the mount to SQLite and the rest to
  // workerd's own fs.
  $workerdFs__deps: ['$FS', '$PATH_FS'],
  $workerdFs__postset: () => addAtPreRun('workerdFs()'),
  $workerdFs: () => {
    Object.defineProperty(FS, 'ignorePermissions', { get: () => true, set() {} });
    // The working directory is the object's, not the isolate process's, so
    // relative paths (which NODERAWFS otherwise leaves to the host's cwd) are
    // resolved here before they reach the fs implementation.
    let cwd = '/';
    FS.cwd = () => cwd;
    FS.chdir = (path) => { cwd = PATH_FS.resolve(path); };
    const resolve = (path) => (typeof path === 'string' && path[0] !== '/' ? PATH_FS.resolve(path) : path);
    const twoPaths = new Set(['renameSync', 'copyFileSync', 'linkSync']);
    const resolving = (impl) => new Proxy(impl, {
      get(target, name) {
        const value = target[name];
        if (typeof value !== 'function' || !String(name).endsWith('Sync')) return value;
        return (first, second, ...rest) =>
          value.call(target, resolve(first), twoPaths.has(name) ? resolve(second) : second, ...rest);
      },
    });
    const nodeFs = fs;
    fs = resolving(nodeFs);
    Object.defineProperty(globalThis, '__pumpkin_fs', {
      configurable: true,
      get: () => nodeFs,
      set: (mounted) => { fs = resolving(mounted); },
    });
  },
});
