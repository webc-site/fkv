# Val á fellivalmynd, breiddin lagar sig að núverandi hlut

Breidd innfæddra `select` er sjálfgefið ákvörðuð af lengsta valmöguleikatextanum. Þegar skipt er um stutta valkosti er of mikið hvítt rými sem hefur áhrif á útlitið.

Þessi hluti nær á gagnsæjan hátt `select` fyrir ofan `b` merkið, sem gerir breiddinni kleift að laga sig nákvæmlega að lengd þess texta sem nú er valinn.

- Viðhalda innfæddum `select` fellivalmyndastílum og aðgengisstuðningi á öllum útstöðvum
- Styður innfæddar gagnvirkar aðgerðir eins og fullkomið lyklaborðsflipaleiðsögn og stefnuskiptaskipti.
- Aðlagaðu breiddina að núverandi valkosti til að útrýma umfram hvítu rými

## Notaðu kynninguna

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```