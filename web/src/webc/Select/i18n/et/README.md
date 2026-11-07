# Rippmenüü valik, laius kohandub praeguse elemendiga

Natiivse `select` laiuse määrab vaikimisi pikim valikutekst. Lühikeste valikute vahetamisel jääb liiga palju valget ruumi, mis mõjutab välimust.

See komponent katab läbipaistvalt natiivse `select` sildi `b` kohal, võimaldades laiusel kohaneda täpselt valitud teksti pikkusega.

- Säilitage kõigis terminalides natiivsed `select` rippmenüü stiilid ja juurdepääsetavuse tugi
- Toetab natiivseid interaktiivseid toiminguid, nagu täielik klaviatuur Tab-navigeerimine ja suunaklahvide vahetamine.
- Liigne valge tühiku eemaldamiseks kohandage laius praegusele valikule

## Kasutage demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```