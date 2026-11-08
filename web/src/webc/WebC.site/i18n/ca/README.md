# Portal d'encaminament que allotja la documentació de la biblioteca de components i el lloc de demostració

- **Disposició de pantalla completa**: el posicionament absolut omple la finestra gràfica, amb desplaçament i navegació integrats.
- **Enrutament dinàmic**: analitzeu els camins d'URL i carregueu dinàmicament demostracions de components o pàgines d'inici.
- **Multi-idioma**: respon i actualitza la barra de navegació i la còpia de la pàgina en temps real.
- **Canvi d'estat**: manté automàticament les classes d'estil de càrrega i pàgina d'inici.

## Utilitzeu la demostració

```html
<c-webc-site></c-webc-site>
```

## classe d'estil

El component canviarà els noms de classe següents dins de si mateix:

- `Ing`: càrrega de ruta o anàlisi del paquet d'idioma.
- `A`: actualment a la pàgina d'inici.