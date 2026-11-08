# Render en toon componentdemonstraties, documentatie en installatie-instructies

- Extraheer en ontleed de titel, inleiding en inhoud van het README-document
- Sandbox om interactieve demo's van componenten uit te voeren en stijlen te isoleren
- Converteer codeblokken in documenten naar interactieve presentaties
- Adaptieve lay-out, breedbeeldkolomweergave, smal schermtabbladweergave

## Gebruik de demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// Initialiseer en verkrijg de bedieningsinterface
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interfaceparameters

### `WebCDemo(el)`

Initialiseer het onderdeel.

- **parameter**
  - `el`:`HTMLElement`, componentcontainer.
- **Retourwaarde**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Stel de demomodule in.
  - `setMd(name, md, lang)`: Stel de componentnaam, de README-documentinhoud en de huidige taal in.
  - `unMount()`: verwijder de component en ruim de gebeurtenislistener op.

### Demomodule `mod` attribuut

- `CSS`:`string`, het stijlbladpad van de component.
- `HTM`:`string`, de HTML-sjabloon van de component.
- `default`:`function(root)`, componentinitialisatiefunctie, ontvangt schaduwrootcontainer.

## stijl klasse

### `.M`

inhoud gebied.

### `.L`

Het linkerpaneel bevat instructies en documentatie.

### `.R`

Rechterpaneel, met daarin de demo.

### `c-nav`

Smalle schermnavigatiebalk.