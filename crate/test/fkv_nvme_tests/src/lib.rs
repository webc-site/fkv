//! 仅包含需要在本地 NVMe / SSD 上执行的隔离测试。
//!
//! 这个 crate 刻意不发布、也不被其它 crate 依赖；`nvme.yml` 只归档它的
//! 测试二进制，避免把整个 workspace 的测试打包上传。
