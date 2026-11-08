# Arquitectura

## temps d'execució

Cada fragment té un fil del sistema operatiu exclusiu i un temps d'execució compio. La sol·licitud entra per la cua d'entrada,
Admissió per màquina d'estat de fragments, completada mitjançant un rebut d'un sol cop.

## emmagatzematge

El motor utilitza l'arbre B+ de còpia sobre escriptura. pàgina de dades, pàgina d'índex, cadena de desbordament i
El manifest es serialitza en fitxers de segment. la branca s'identifica amb `(branch, term)` i la té cadascuna
manifest i instantània de mapeig.

## IO

`fkv_io` Defineix el tret d'IO asíncron. `fkv_io_compio` Implementat amb compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` proporciona memòria i backends d'injecció d'errors, `fkv_object_s3` proporciona
Backend d'objectes S3.

## persistència

El manifest registra la pàgina arrel, l'arrel TTL, les metadades de la branca i el mapeig. Quan Reobri
Primer reprodueix el manifest, després reconstrueix el mapeador de pàgines i, a continuació, comença a servir les sol·licituds.