# 🛡️ Physical Isolation & Sandbox Configuration

## Overview

LocalPersona spawns `llama-server` as a child process for LLM inference. By default, this process runs with the same OS privileges as the main Tauri application. For maximum security, especially in multi-user environments or when running untrusted models, physical isolation is recommended.

## Isolation Levels

### Level 1: cgroups v2 Memory Limits (Recommended)

The simplest isolation that prevents the LLM server from consuming all system memory.

```bash
# Run LocalPersona with a 4GB memory limit
systemd-run --scope \
    -p MemoryMax=4G \
    -p MemoryHigh=3G \
    ./target/release/localpersona

# Monitor memory usage
./scripts/cgroup_monitor.sh 10240 5
```

**What this does:**
- Limits the total memory (RSS) to 4GB
- Triggers OOM kill if the limit is exceeded
- The memory monitor script can trigger Preemptive Reset before OOM

### Level 2: Linux Namespaces (Strong Isolation)

Run the LLM server in isolated namespaces for filesystem, network, and PID isolation.

```bash
# Create isolated namespace for llama-server
unshare --mount --pid --fork --net -- \
    /usr/local/bin/llama-server \
    --model /path/to/model.gguf \
    --port 8080 \
    --host 127.0.0.1
```

**What this does:**
- Separate mount namespace (can't see host filesystem)
- Separate PID namespace (can't see host processes)
- Separate network namespace (no network access)
- Process can't escape to host

### Level 3: gVisor (Container Runtime)

Use gVisor's `runsc` as the container runtime for the LLM server.

```dockerfile
# Dockerfile for llama-server sandbox
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y llama-server
COPY models/ /models/
EXPOSE 8080
CMD ["llama-server", "--model", "/models/model.gguf", "--port", "8080"]
```

```bash
# Run with gVisor
docker run --runtime=runsc \
    --memory=4g \
    --cpus=2 \
    --network=none \
    -v /path/to/models:/models:ro \
    llama-server-sandbox
```

**What this does:**
- gVisor intercepts all system calls (Sentry + Gofer)
- Container can't access host kernel directly
- Memory and CPU limits enforced by the container runtime
- Read-only model mount

### Level 4: Firecracker microVM (Maximum Isolation)

The strongest isolation: a full VM with its own kernel.

```bash
# Create Firecracker VM for llama-server
firecracker --api-sock /tmp/firecracker.sock

# Configure VM
curl --unix-socket /tmp/firecracker.sock -X PUT \
    http://localhost/boot-source \
    -d '{"kernel_image_path": "./vmlinux", "boot_args": "..."}'

curl --unix-socket /tmp/firecracker.sock -X PUT \
    http://localhost/machine-config \
    -d '{"vcpu_count": 4, "mem_size_mib": 4096}'

# Start VM
curl --unix-socket /tmp/firecracker.sock -X PUT \
    http://localhost/actions \
    -d '{"action_type": "InstanceStart"}'
```

**What this does:**
- Full hardware-level isolation
- Separate kernel, separate memory space
- VM escape is extremely difficult
- Near-native performance with KVM

## Recommended Configuration

| Environment | Isolation Level | Memory Limit | Performance |
|-------------|----------------|--------------|-------------|
| Personal use | Level 1 (cgroups) | 4-8 GB | Native |
| Development | Level 2 (Namespaces) | 4 GB | Native |
| Production | Level 3 (gVisor) | 4 GB | ~95% native |
| High security | Level 4 (Firecracker) | 4 GB | ~98% native |

## Integration with LocalPersona

The app supports cgroups v2 monitoring out of the box:

1. Start the memory monitor: `./scripts/cgroup_monitor.sh`
2. The app reads `/tmp/localpersona-preemptive-reset` for signals
3. When a Preemptive Reset is triggered, the LLM server is respawned

For higher isolation levels, the app would need to be modified to spawn `llama-server` inside a container or VM instead of as a direct child process. This is planned for a future release.

## Security Considerations

- **Model files**: Untrusted GGUF models could contain malicious code. Running in a sandbox prevents model exploits from affecting the host.
- **Network isolation**: The LLM server should only listen on localhost. Use `--host 127.0.0.1` and network namespaces.
- **Filesystem isolation**: The LLM server should only have read access to model files. Use read-only mounts.
- **Resource limits**: Always set memory and CPU limits to prevent DoS.
