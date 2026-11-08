# HTML සහ JS කේතය මාර්ගගතව දෝෂහරණය කිරීම

- කේතයට පහළින් නිදොස් කිරීමේ තීරුවක් ලබා දී CodePen සහ JSFiddle නිදොස් කිරීමේ ද්වාර සපයන්න
- තෝරාගත් දෝශ නිරාකරණ වේදිකාව ස්වයංක්‍රීයව මතක තබා ගන්න
- HTML, CSS සහ JS කේතය ගතිකව ජනනය කිරීමට `.gen` ගුණාංගය හරහා ආපසු ඇමතුම් එන්නත් කිරීමට සහය දක්වයි
- `language-js` පන්තියේ නම සහිත ළමා මූලද්‍රව්‍ය ස්වයංක්‍රීයව හඳුනාගෙන JS මාදිලියේ ධාවනය කරන්න

## Demo භාවිතා කරන්න

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // ආපසු [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## අතුරුමුහුණත් පරාමිතීන්

### ගුණාංග සහ ක්රම

- `.gen`: එන්නත් කළ ආපසු කැඳවීමේ කාර්යය. ** දෝශ නිරාකරණ තීරුව දර්ශනය වීම සඳහා මෙම ආපසු කැඳවීම එන්නත් කළ යුතුය. **
  - පරාමිතිය:
    - `type`: එන්නත් කළ අන්තර්ගත වර්ගය (`HTML = 1`, `JS = 2`).
    - `text`: ළමා මූලාංගයෙන් උපුටා ගත් කේත පෙළ.
  - ප්‍රතිලාභ අගය: `[htm, css, js]`