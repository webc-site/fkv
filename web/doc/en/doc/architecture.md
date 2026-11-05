# Architecture

## Runtime

Each shard has an exclusive OS thread and a compio runtime. The request enters through the ingress queue,
Admission by shard state machine, completed through oneshot receipt.

## Storage

The engine uses copy-on-write B+tree. data page, index page, overflow chain and
The manifest is serialized into segment files. branch is identified by `(branch, term)` and is held by each
manifest and mapping snapshot.

## IO

`fkv_io` Defines the asynchronous IO trait. `fkv_io_compio` Implemented with compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` provides memory and fault injection backends, `fkv_object_s3` provides
S3 object backend.

## Persistence

The manifest records root page, TTL root, branch metadata and mapping. When Reopen
First replay the manifest, then rebuild the page mapper, and then start serving requests.