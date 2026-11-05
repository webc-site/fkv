# Rullgardinsval, bredd anpassar sig till aktuellt objekt

Bredden på inbyggd `select` bestäms av den längsta alternativtexten som standard. När du byter korta alternativ blir det för mycket vitt utrymme, vilket påverkar utseendet.

Den här komponenten täcker transparent `select` ovanför `b`-taggen, vilket gör att bredden kan anpassas till längden på den markerade texten.

- Behåll inbyggda `select` rullgardinsmenystilar och tillgänglighetsstöd på alla terminaler
- Stöder inbyggda interaktiva operationer som komplett tangentbordsnavigering och växling av riktningsknappar.
- Anpassa bredden till det aktuella alternativet för att eliminera överflödigt vitt utrymme

## Använd demon

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```