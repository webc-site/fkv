# Upau wa kusogeza wa juu ambao huporomoka au kupanuka kiotomatiki ukurasa unaposonga

- **Ficha Kiotomatiki**: Ficha kiotomatiki unaposogeza chini ya ukurasa.
- **Panua kiotomatiki**: Huonyeshwa kiotomatiki ukurasa unaposogezwa juu au upau wa kusogeza unapotoweka.
- **Uboreshaji wa Utendaji**: Tumia `requestAnimationFrame` kusukuma usogezaji.
- **Nyenzo za matoleo**: Wasikilizaji na waangalizi wa tukio huondolewa kiotomatiki kipengele kinapoharibiwa.

## Tumia onyesho

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Kiungo</a>
      </c-nav>
    </nav>
    <div class="content">Telezesha kidole chini ili kuficha urambazaji, telezesha kidole juu ili kuionyesha</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Maagizo ya matumizi

Lazima kitumike kama mtoto au kipengele cha kizazi cha `<c-vs>`.

## hali ya mtindo

Kijenzi hiki hubadilisha jina la darasa lifuatalo kwenye `nav.Nav`:

- `D`: Onyesha hali (telezesha chini au juu).
- `I`: Hali ya mpito inafichwa.
- `H`: Hali iliyofichwa.

## vigezo vya mtindo

- `--top`: Imesasishwa kiotomatiki na kijenzi hadi thamani hasi ya urefu, inayotumika kwa kuweka na kugeuza.