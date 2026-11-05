# Navigacijos meniu pritaikytas skirtingiems ekrano dydžiams

- Darbalaukis: nuorodos rodomos horizontaliose plytelėse
- Mobilioji versija: sutraukti į meniu mygtuką, spustelėkite, kad išstumtumėte šoninės juostos stalčių
- Maršruto aktyvinimas: automatiškai palyginkite dabartinį kelią ir maišos reikšmę ir pridėkite aktyvinimo būsenos klasės pavadinimą prie atitinkamos nuorodos `A`
- Automatiškai uždaryti: mobiliajam terminalui spustelėjus nuorodą arba išorinį šoninės juostos foną, šoninė juosta automatiškai užsidaro.

## naudoti

```html
<c-nav-menu>
  <a href="/">pirmas puslapis</a>
  <a href="/doc">dokumentas</a>
  <a href="/about">apie</a>
</c-nav-menu>
```

## iliustruoti

Kai komponentas inicijuojamas, jei antriniame elemente nėra `c-side`, bus automatiškai sukurtas mobiliojo meniu mygtukas ir `c-side` šoninė juosta, o visos `a` žymos bus perkeltos į šoninę juostą.

## stiliaus klasė

- **A**: suaktyvinta nuoroda automatiškai pridės klasės pavadinimą `A`