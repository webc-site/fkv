# Pudotusvalikon valintaruudun ja painikkeen yhdistelmä

- Liitä valintaruutu ja painike vaakasuoraan
- Tallenna valittu tila automaattisesti localStorageen
- Keskeytä painikkeen napsautukset ja lähetä mukautettuja tapahtumia

## Käytä demoa

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Vaihtoehto 1</option>
    <option value="option2">Vaihtoehto 2</option>
  </c-select>
  <button>juosta</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## omaisuutta

- `key`: avaimen nimi, jota käytetään paikallisen tallennustilan pysyvyyttä varten
- `value`: tällä hetkellä valittu arvo

## tapahtuma

- `submit`: lähetetään, kun painiketta napsautetaan, `e.value` sisältää tällä hetkellä valitun arvon