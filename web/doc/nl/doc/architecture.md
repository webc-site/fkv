# Architectuur

## looptijd

Elke Shard heeft een exclusieve OS-thread en een compio-runtime. Het verzoek komt binnen via de ingangswachtrij,
Toelating via een Shard State-machine, voltooid via een eenmalige ontvangstbevestiging.

## opslag

De engine maakt gebruik van copy-on-write B+tree. gegevenspagina, indexpagina, overloopketen en
Het manifest wordt geserialiseerd in segmentbestanden. branch wordt geïdentificeerd door `(branch, term)` en wordt door elk beheerd
manifest en mapping-snapshot.

## IO

`fkv_io` Definieert de asynchrone IO-eigenschap. `fkv_io_compio` Geïmplementeerd met compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` biedt backends voor geheugen en foutinjectie, `fkv_object_s3` biedt
S3-object-backend.

## vasthoudendheid

Het manifest registreert de rootpagina, TTL-root, branch-metagegevens en mapping. Wanneer heropenen
Speel eerst het manifest opnieuw af, bouw vervolgens de paginatoewijzer opnieuw op en begin vervolgens met het verwerken van verzoeken.