# Għażla drop-down, il-wisa 'tadatta għall-oġġett kurrenti

Il-wisa' ta' `select` indiġeni hija determinata mill-itwal għażla test b'mod awtomatiku. Meta taqleb għażliet qosra, hemm wisq spazju abjad, li jaffettwa d-dehra.

Dan il-komponent ikopri b'mod trasparenti l-`select` indiġeni 'l fuq mit-tikketta `b`, li jippermetti li l-wisa' tadatta b'mod preċiż għat-tul tat-test magħżul bħalissa.

- Żomm stili indiġeni tal-menu dropdown u appoġġ għall-aċċessibbiltà fit-terminals kollha
- Jappoġġja operazzjonijiet interattivi indiġeni bħal navigazzjoni kompluta tat-Tab tat-tastiera u swiċċjar taċ-ċavetta tad-direzzjoni.
- Adatta l-wisa 'għall-għażla attwali biex telimina l-ispazju abjad żejjed

## Uża d-demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```