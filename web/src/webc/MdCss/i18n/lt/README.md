# Tipografijos stiliai, optimizuoti skaitymo patirčiai

- **Pagrindinis rinkimas**: prisitaikykite prie pavadinimų, pastraipų, pusjuodžio, kursyvo, perbraukimo, įterpto kodo, paveikslėlių ir skiriamųjų linijų.
- **Nuorodos animacija**: užvedus pelės žymeklį rodoma pabraukimo animacija.
- **Projektų sąrašas**: nesutvarkytuose sąrašuose naudojami ženkleliai, sutvarkytuose sąrašuose naudojami suapvalinti fono serijos numeriai ir palaikomas kelių lygių įdėjimas.
- **Užduočių sąrašas**: pateikia žymės langelio stilių, palaiko užduočių ir atliktų būseną.
- **Įspėjimo patarimai**: palaiko penkių tipų įspėjimo laukelius: Pastaba, Patarimas, Svarbus, Įspėjimas ir Dėmesio.
- **Lentelės ir kodas**: pateikia lentelės eilučių paryškinimą ir kodų blokus su šešėliais ir gradiento fonais.

## Naudokite demonstracinę versiją

```html
<div class="Md">
  <h1>titulą</h1>
  <p>Pastraipos tekstas ir <a href="#">nuoroda</a></p>
  <hr>
  <h2>Antrojo lygio titulas</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Užduoties elementas</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Pastabų turinys</p>
  </blockquote>
</div>
```

## stiliaus klasė

Stiliai taikomi HTML elementams po `.Md` konteineriu.

### Įspėjimo dėžutės stilius

Pridėkite šį klasės pavadinimą prie `blockquote` elemento su `.q` klasės pavadinimu, kad perjungtumėte įspėjimo laukelį:

- `note`: pastabos
- `tip`: patarimas
- `important`: Svarbu
- `warning`: įspėjimas
- `caution`: Pastaba