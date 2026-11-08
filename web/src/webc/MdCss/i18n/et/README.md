# Lugemiskogemuse jaoks optimeeritud tüpograafiastiilid

- **Põhiladu**: kohandamine pealkirjade, lõikude, paksu kirja, kaldkirja, läbikriipsutamise, tekstisisese koodi, piltide ja eraldusjoontega.
- **Lingi animatsioon**: kuvage kursorit hõljutades allajoonitud animatsiooni.
- **Projektiloend**: järjestamata loendid kasutavad täppe, järjestatud loendid kasutavad ümardatud taustal olevaid seerianumbreid ja toetavad mitmetasandilist pesastamist.
- **Ülesannete loend**: pakub märkeruudu stiili, toetab ülesandeid ja lõpetatud olekut.
- **Hoiatusnõuanded**: toetab viit tüüpi hoiatuskaste: Märkus, Vihje, Tähtis, Hoiatus ja Tähelepanu.
- **Tabelid ja kood**: pakub tabeliridade kursorit esiletõstmist ning koodiplokke varjude ja gradiendi taustaga.

## Kasutage demo

```html
<div class="Md">
  <h1>pealkiri</h1>
  <p>Lõigu tekst ja <a href="#">link</a></p>
  <hr>
  <h2>Teise taseme tiitel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Ülesandeüksus</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Märkige sisu</p>
  </blockquote>
</div>
```

## stiiliklass

Stiile rakendatakse konteineri `.Md` all olevatele HTML-elementidele.

### Hoiatuskasti stiil

Lisage elemendile `blockquote` järgmine klassi nimi koos `.q` klassi nimega, et hoiatuskasti sisse lülitada:

- `note`: märkmed
- `tip`: vihje
- `important`: oluline
- `warning`: hoiatus
- `caution`: Märkus