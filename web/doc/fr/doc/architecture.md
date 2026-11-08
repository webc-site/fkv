# Architecture

## durée d'exécution

Chaque fragment possède un thread de système d'exploitation exclusif et un runtime Compio. La requête entre via la file d'attente d'entrée,
Admission par machine à états de fragments, complétée par un reçu unique.

## stockage

Le moteur utilise la copie sur écriture B+tree. page de données, page d'index, chaîne de débordement et
Le manifeste est sérialisé en fichiers de segments. la branche est identifiée par `(branch, term)` et est détenue par chacun
manifeste et instantané de mappage.

## IO

`fkv_io` Définit le trait IO asynchrone. `fkv_io_compio` Implémenté avec compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` fournit des backends de mémoire et d'injection de pannes, `fkv_object_s3` fournit
Backend d’objet S3.

## persistance

Le manifeste enregistre la page racine, la racine TTL, les métadonnées de branche et le mappage. Lors de la réouverture
Relisez d’abord le manifeste, puis reconstruisez le mappeur de page, puis commencez à traiter les requêtes.