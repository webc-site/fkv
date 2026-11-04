# 05 分裂启发式：0.10 fill ratio + 全量 index 重建 ≠ C++

**严重度**：P0（算法分歧，影响空间/写放大）

## 现象

Rust 在 `upsert_group` 里：任意非 root 子页 `fill_ratio < 0.10` 时，触发 `rebuild_index_all` —— **自底向上重建整棵 index 树**（`crate/engine/fkv_task_write/src/lib.rs:196-208、417-434`）。

`fill_ratio = content_len / page_size`（lib.rs:520、548）。

## C++ 对照

C++ 里**不存在 0.10 这个常量**。真实启发式是 1/4、3/4 四分页重分配：

- data page：新区建好后，若 `cur_len < data_page_size/4` 且 `prev.ContentLength() > 3*data_page_size/4` 且 `prev.RestartNum() > 1`，与前驱重分配（`src/tasks/batch_write_task.cpp:1064-1095`）。
- index page 同理（`:982-1019`）。

即 C++ 是**局部重平衡**，Rust 是**全局重建**。

## 后果

- 写放大：一次不满触发的全树重建，在写密集负载下可能放大成 O(tree) 每批。
- 页布局与 C++ 不同 → 空间放大（amplification）指标对不上。
- COW 语义下全树重建会大量分配新 file page，加剧 GC 压力。

## 建议

1. 用 C++ 的四分页重分配替换 0.10 阈值逻辑。
2. 去掉 `rebuild_index_all` 热路径调用（或降级为显式的 compaction 步骤）。
3. 补 V3 操作差分：同一 op log 后比较页数 / 空间放大。

## 验收

- 同一 workload 页数与 C++ 同量级。
- 无全树重建触发的写放大尖峰。
