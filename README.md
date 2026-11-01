# farula

El telón de discada. Al cambiar de wallpaper o de look, congela la pantalla,
deja que todo cambie por debajo (wallpaper, barra, bordes, kitty) y lo revela
de un solo golpe con un efecto, en el color nuevo. También abre el escritorio
al desbloquear.

```
farula freeze [--image f.ppm] [--hold]   congelar (--image: esta imagen en vez
                                         de una foto; --hold: sin tope de 15 s)
farula reveal [efecto]                   abrir
farula demo [efecto]                     congelar y abrir, para verlo
farula list                              los efectos
farula active                            sale con 0 si hay telón puesto
farula meneo [segundos]                  en la terminal: la discada meneándose
                                         con la cuchara (al empezar a actualizar)
farula lista [segundos]                  en la terminal: el vaquero bailando
                                         (al terminar; DISCADA_SIN_ARTE lo quita)
```

Efectos: iris, quemado, glitch, vidrio, derretido, fade, crt, azulejos,
cartas, onda, tinta, wipe, pixelado (o `random`). Cada look elige el suyo en
su `look.toml`:

```toml
[farula]
fx = "derretido"            # o una lista: ["vidrio", "onda"], o "random"
```

Sin el efecto, `freeze` sale con 1 y los scripts usan la transición de awww
de siempre; `touch ~/.config/hypr/farula.off` lo apaga. El telón no agarra
clics y, sin `--hold`, se abre solo a los 15 s. `FARULA_DEBUG=1` deja los fps
de cada efecto en `$XDG_RUNTIME_DIR/farula.log`.

```
cargo install --git https://github.com/daniiiqa/farula --root ~/.local --locked
```

MIT.
