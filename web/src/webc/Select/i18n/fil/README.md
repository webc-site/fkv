# Drop-down na pagpipilian, ang lapad ay umaangkop sa kasalukuyang item

Ang lapad ng katutubong `select` ay tinutukoy ng pinakamahabang teksto ng opsyon bilang default. Kapag lumilipat ng mga maikling opsyon, mayroong masyadong maraming puting espasyo, na nakakaapekto sa hitsura.

Malinaw na sinasaklaw ng component na ito ang native na `select` sa itaas ng tag na `b`, na nagpapahintulot sa lapad na tumpak na umangkop sa haba ng kasalukuyang napiling text.

- Panatilihin ang katutubong `select` mga istilo ng dropdown na menu at suporta sa pagiging naa-access sa lahat ng mga terminal
- Sinusuportahan ang mga katutubong interactive na operasyon tulad ng kumpletong pag-navigate sa tab ng keyboard at pagpapalit ng key ng direksyon.
- Ibagay ang lapad sa kasalukuyang opsyon para maalis ang sobrang puting espasyo

## Gamitin ang demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```