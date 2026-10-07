---
status: accepted
---

# Simulateur pas à pas : chaque siège de l'arène suspend la main à sa décision

Le client local (ticket #12) fait jouer un humain contre des bots du simulateur. Or l'arène de `rs_poker` joue une main d'un bloc : elle appelle elle-même chaque siège et ne rend la main qu'à la fin. Un humain, lui, répond au rythme de ses clics, depuis la boucle d'événements de la fenêtre. **Décision : chaque siège de l'arène suspend la main à sa décision.** `NitroGame` reprend alors la main et la fait avancer pas à pas : il répond lui-même pour un siège qui a une stratégie, et attend `act` pour un siège externe. `play_hand` et `play_to_end` ne sont plus que des boucles sur ces pas.

Ce point modifie l'ADR 0002, selon lequel « une main qui se suspendrait fait paniquer le simulateur ». La suspension devient le fonctionnement normal.

## Contexte

- L'arène ne publie pas de pas de mise. `run_betting_round` et `run_single_agent` sont privés, et `run_round` joue un tour d'enchères entier en appelant les agents.
- Les agents de l'arène sont `async`. Un futur qui rend `Pending` suspend la main sans rien perdre de son état (donne, pots, ordre de parole).
- Un humain ne peut pas être un `SeatStrategy` : `decide` doit répondre tout de suite, et bloquer un thread en attendant un clic mélangerait la boucle de jeu et celle de la fenêtre.

## Décision

- **Le futur de la main est gardé entre deux pas.** La simulation y est déplacée (`async move`), et le futur rend la simulation terminée. Chaque pas le sonde une fois avec `Waker::noop`. Il n'y a pas de runtime tokio.
- **Un siège de l'arène ne décide jamais.** Il dépose une copie du `GameState` dans un échange partagé et rend `Pending` tant qu'aucune réponse n'y est déposée. `NitroGame` en tire la `SeatView` du siège à parler.
- **Qui répond** :
  - `Seat::Strategy` : `step` demande la décision à la stratégie, avec le générateur aléatoire du siège, comme avant.
  - `Seat::External` : `step` rend `AwaitingExternal` sans rien changer, et `act` répond. `act` refuse une décision hors de `SeatView::legal_decisions`.
- **Un seul chemin.** Les parties entre bots et les parties avec un humain passent par les mêmes pas. Les générateurs aléatoires sont tirés dans le même ordre qu'avant : à graine égale, le simulateur rend les mêmes rapports, au bit près.
- **Ce que voit un client** : `table_view(siège)` donne la table vue depuis un siège. On y voit ses propres cartes, l'état public, et les cartes abattues une fois la main finie. Le client n'a donc pas à décider ce qu'il peut montrer.

## Conséquences

- Chaque décision coûte une copie du `GameState` et un verrou de plus. C'est négligeable : 100 000 parties entre bots aléatoires tournent en moins d'une seconde sur 8 threads.
- Un client n'est qu'une couche d'affichage et de saisie. Il appelle `step` pour faire avancer la partie, `table_view` pour l'afficher et `act` pour l'humain.
- Les actions illégales sont refusées par le simulateur lui-même, pas seulement masquées par l'interface.
- Si deux joueurs sont à tapis avant la fin des enchères, l'arène distribue le reste du tableau sans décision intermédiaire. Le client passe donc directement à la main terminée.
