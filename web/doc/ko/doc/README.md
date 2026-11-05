# fkv

fkv는 순수 Rust 내장형 KV 스토리지 엔진입니다.

## 특성

- 샤드당 하나의 OS 스레드, 협력 작업 루프.
- COW B+트리, 추가 전용 세그먼트 IO.
- 페이지/인덱스/매니페스트/오버플로/압축 형식.
- 로컬/메모리/시뮬레이션/S3 객체 백엔드.
- 독립형/클라우드/대기 복제 모드.
- C ABI도 없고 Python SDK도 없습니다. 오직 Rust만이 공개 인터페이스입니다.

## 입구

- [건축학](architecture.md)
- [Crate](crates.md)
- [시험](testing.md)
- [성능](performance.md)