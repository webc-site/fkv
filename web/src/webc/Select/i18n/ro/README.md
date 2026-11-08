# Selecție drop-down, lățimea se adaptează la elementul curent

Lățimea lui `select` nativ este determinată implicit de cel mai lung text al opțiunii. Când schimbați opțiunile scurte, există prea mult spațiu alb, ceea ce afectează aspectul.

Această componentă acoperă în mod transparent `select` nativ deasupra etichetei `b`, permițând lățimii să se adapteze cu precizie la lungimea textului selectat în prezent.

- Mențineți stilurile native de meniu drop-down `select` și suportul pentru accesibilitate pe toate terminalele
- Suportă operațiuni interactive native, cum ar fi navigarea completă a tastaturii și comutarea tastelor de direcție.
- Adaptați lățimea la opțiunea curentă pentru a elimina excesul de spațiu alb

## Utilizați demonstrația

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```