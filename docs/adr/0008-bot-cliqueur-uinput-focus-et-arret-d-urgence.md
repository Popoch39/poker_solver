---
status: accepted
---

# Bot cliqueur : clics uinput, vérification du focus avant chaque clic et arrêt d'urgence

Le ticket #14 fait jouer le bot cliqueur sur le client local : il lit l'état à l'écran (ticket #13), choisit son action avec le héros solver (ADR 0007) et clique sur le bouton correspondant. Un clic injecté agit sur le bureau entier, pas sur une fenêtre : un clic au mauvais moment peut atterrir dans n'importe quelle application. **Décision : les clics passent par un pointeur virtuel uinput ; avant chaque clic, le bot vérifie que la fenêtre active est le client local, à sa taille ; un arrêt d'urgence, jamais levé tout seul, coupe toute injection.**

## Injection

- **uinput, comme le prévoit le spec.** Le bot crée un pointeur absolu (axes X/Y sur tout l'agencement des écrans, bouton gauche), que udev classe en souris et libinput en mouvement absolu. Il déplace le pointeur sur le bouton, puis presse et relâche le bouton gauche. Une pression est toujours relâchée.
- **Permissions.** `/dev/uinput` n'est accessible qu'à root. Une règle udev `TAG+="uaccess"` le donne à l'utilisateur de la session locale active. Elle est documentée dans la crate, jamais appliquée par elle, et doit être retirée quand le bot ne sert pas : n'importe quel programme de cet utilisateur peut alors injecter des entrées.
- **Pas de backend wlr virtual-pointer.** Il n'aurait pas besoin de root, mais la règle udev suffit déjà pour se passer de root. Un second backend doublerait le code qu'on ne peut tester qu'en vrai. Le trait `Injector` lui laisse la place.
- **Coordonnées.** Le point d'un bouton dans la fenêtre (`LAYOUT`) devient un point de l'écran par la position de la fenêtre active (`at` de `hyprctl activewindow -j`). Le client doit flotter à sa taille, sans mise à l'échelle (règles Hyprland du client local).

## Ne cliquer que sur le client local

Avant chaque clic, et pas seulement au démarrage :

- exactement une fenêtre porte le titre du client, et c'est bien le client (app_id) : sinon, un sosie est sur le bureau ;
- c'est la fenêtre active (même adresse, titre et classe) ;
- elle a la taille du client, sinon les boutons ne sont plus où le `LAYOUT` les place.

Si une seule condition manque, le bot ne clique pas et s'arrête. Il ne tente rien d'autre : il ne refocalise pas la fenêtre et ne déplace rien.

Un état déjà cliqué n'est pas recliqué tant que le client n'a pas dessiné l'effet du clic. Si rien ne change dans le délai de patience, le bot s'arrête au lieu de recliquer.

## Arrêt d'urgence

- Il est levé par SIGINT, par SIGTERM ou par l'apparition d'un fichier (`$XDG_RUNTIME_DIR/nitro-bot.stop`), qu'un raccourci du bureau peut créer. Un second signal termine le processus.
- Il est vérifié avant chaque clic, de nouveau après la vérification du focus, et entre le déplacement du pointeur et la pression.
- Il ne retombe jamais : `play` refuse de démarrer tant que le fichier existe.

## Vérification hors écran

Le client local rend ses images hors écran, le bot les lit, décide et « clique » via un injecteur qui transmet le clic au gestionnaire de clics du client. Les mêmes parties sont jouées par le héros directement dans le simulateur (`compare_with` : même graine, mêmes donnes). Deux mesures :

- **Concordance** : chaque décision du bot est comparée à celle du héros, calculée sur la vue que le simulateur donne au héros, avec le même tirage aléatoire.
- **Taux de victoire** : celui du bot et celui du héros simulé, et leur écart apparié, partie par partie.

## Conséquences

- La concordance mesurée est de 100 % : le lecteur ne fait pas d'erreur sur les images du client. Le taux de victoire du bot reste dans l'intervalle de confiance du héros simulé (voir le test lent `tournaments`).
- Les tirages aléatoires du bot ne sont pas ceux du simulateur, qui les tire du générateur du siège. Une stratégie mixte fait donc diverger les deux parties dès son premier tirage différent, et l'écart apparié n'est plus nul.
- Le chemin réel (`hyprctl`, `grim`, uinput) n'a pas été essayé en vrai : aucun test n'injecte d'entrée ni ne capture l'écran.
