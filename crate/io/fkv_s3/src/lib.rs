//! S3 客户端：SigV4 签名 + compio 上的 HTTP/1.1。
//!
//! 拆分目的：原实现基于 `object_store`，而它内部绑定 tokio。这里不再依赖任何
//! 异步运行时（签名是纯函数），传输层只用 compio，符合「核心路径只用 compio」
//! 的约束。

pub mod client;
pub mod http;
pub mod sig;
