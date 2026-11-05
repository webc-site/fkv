# fkv

fkv යනු පිරිසිදු රස්ට් කාවැද්දූ KV ගබඩා එන්ජිමකි.

## ලක්ෂණය

- එක් කොටසකට OS නූල් එකක්, සමුපකාර වැඩ ලූපයක්.
- COW B+tree, append-only segment IO.
- page/index/manifest/overflow/compression Format.
- දේශීය/මතකය/සමාකරණ/S3 වස්තු පසුබිම.
- ස්වාධීන / වලාකුළු / පොරොත්තු අනුරූ මාදිලිය.
- C ABI නැත, Python SDK නැත; රස්ට් යනු පොදු අතුරු මුහුණත පමණි.

## ඇතුල්වීම

- [ගෘහ නිර්මාණ ශිල්පය](architecture.md)
- [Crate](crates.md)
- [පරීක්ෂණය](testing.md)
- [කාර්ය සාධනය](performance.md)