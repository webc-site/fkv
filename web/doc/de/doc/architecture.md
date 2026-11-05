# Architektur

## Laufzeit

Jeder Shard verfügt über einen exklusiven Betriebssystem-Thread und eine Compio-Laufzeit. Die Anfrage wird über die Eingangswarteschlange eingegeben.
Zulassung per Shard-State-Machine, abgeschlossen durch One-Shot-Empfang.

## Lagerung

Die Engine verwendet Copy-on-Write B+tree. Datenseite, Indexseite, Überlaufkette und
Das Manifest wird in Segmentdateien serialisiert. Der Zweig wird durch `(branch, term)` identifiziert und von jedem gehalten
Manifest und Mapping-Snapshot.

## IO

`fkv_io` Definiert das asynchrone E/A-Merkmal. `fkv_io_compio` Implementiert mit compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` stellt Speicher und Fault-Injection-Backends bereit, `fkv_object_s3` stellt bereit
S3-Objekt-Backend.

## Beharrlichkeit

Das Manifest zeichnet Stammseite, TTL-Stamm, Zweigmetadaten und Zuordnung auf. Bei Wiedereröffnung
Spielen Sie zuerst das Manifest ab, erstellen Sie dann den Page Mapper neu und beginnen Sie dann mit der Bearbeitung von Anforderungen.