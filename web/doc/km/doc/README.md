# fkv

fkv គឺជាម៉ាស៊ីនផ្ទុក KV ដែលបង្កប់ដោយច្រែះសុទ្ធ។

## លក្ខណៈ

- ខ្សែស្រឡាយ OS មួយក្នុងមួយ shard រង្វិលជុំការងារសហការ។
- COW B+tree, ផ្នែកបន្ថែមតែ IO ។
- page/index/manifest/overflow/compression format។
- local/memory/simulated/S3 object backend។
- របៀប​ចម្លង​ដោយ​ឯកឯង / ពពក / រង់ចាំ។
- គ្មាន C ABI គ្មាន Python SDK; មានតែ Rust គឺជាចំណុចប្រទាក់សាធារណៈ។

## ច្រកចូល

- [ស្ថាបត្យកម្ម](architecture.md)
- [Crate](crates.md)
- [សាកល្បង](testing.md)
- [ការសម្តែង](performance.md)