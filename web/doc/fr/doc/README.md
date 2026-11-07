# fkv

fkv est un pur moteur de stockage KV intégré à Rust.

## caractéristiques

- Un thread de système d'exploitation par fragment, boucle de travail coopérative.
- COW B+tree, ajout uniquement du segment IO.
- format page/index/manifeste/débordement/compression.
- backend d'objet local/mémoire/simulé/S3.
- mode de réplique autonome/cloud/veille.
- Pas de C ABI, pas de SDK Python ; seul Rust est l'interface publique.

## Entrée

- [Architecture](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [performance](performance.md)