---
status: accepted
---

# Simulateur : boucle de tournoi maison autour de l'arène `rs_poker`, pas son `SingleTableTournament`

Le spec #1 construit le simulateur sur l'arène de `rs_poker`. Le tournoi tout fait de l'arène (`SingleTableTournament`) ne sait pas jouer un Expresso Nitro : blindes fixes et heads-up faux. **Décision : chaque main est jouée par l'arène (`HoldemSimulation`), mais la boucle de tournoi est écrite dans le crate `nitro-simulator`.** Elle gère la montée des blindes, le bouton, les éliminations et les places.

## Contexte

Constats sur `rs_poker` 5.1.0 (notes d'exploration du 07/10/2026, vérifiés par un programme de test) :

- **Blindes fixes.** `SingleTableTournament` reconstruit chaque main avec les mêmes blindes. Il n'existe ni structure de niveaux ni compteur de mains. La structure Nitro (33 niveaux d'une minute, 10/20 → 25000/50000) est donc impossible à jouer avec lui.
- **Heads-up faux.** Après une élimination, la table garde 3 sièges dont un à 0 jeton. L'arène n'inverse l'ordre des blindes que si la table a 2 sièges : le bouton poste alors la grosse blinde. Effet de bord : l'export Open Hand History perd les cartes des joueurs.
- **Graine.** La donne d'une `HoldemSimulation` est reproductible avec `build_with_rng`. En revanche, plusieurs chemins utilisent le générateur du thread et ignorent toute graine : `MonteCarloGame`, `FlatDeck::sample`, l'ICM simulé et les agents CFR.
- **Async.** Les agents et l'arène sont `async` (tokio côté `rs_poker`).

## Décision

- **On utilise** :
  - `HoldemSimulation` pour jouer chaque main : donne, blindes, enchères, pots annexes, abattage. On lui passe un `StdRng` tiré de la graine de la partie.
  - L'évaluateur de mains, à travers l'arène.
  - Les types `Card` et `GameState`.
- **On n'utilise pas** :
  - `SingleTableTournament`.
  - Les agents fournis (`RandomAgent`, etc.). Un siège est un `SeatStrategy` maison, adapté en `Agent` pour l'arène. Il ne voit que ses propres cartes et reçoit un générateur aléatoire tiré de la graine.
- **Boucle maison** :
  - Le niveau dépend du nombre de mains jouées : 4 mains par niveau à trois, 5 en heads-up, deux hypothèses configurables tirées du dataset NitroVariance.
  - En heads-up, la table est reconstruite avec **2 sièges**, ce qui corrige l'ordre des blindes.
  - Si deux joueurs sont éliminés sur la même main, le plus petit tapis de départ finit derrière.
- **Pas de runtime tokio.** Nos stratégies décident sans attendre et aucun historien n'est branché, donc le futur d'une main se termine dès le premier `poll` (`Waker::noop`). Une main qui se suspendrait fait paniquer le simulateur, au lieu de le bloquer en silence.
- **Reproductibilité** :
  - La partie *i* est jouée avec une graine dérivée de (graine, *i*).
  - Les résultats sont agrégés dans l'ordre des parties, si bien que le rapport est identique au bit près quel que soit le nombre de threads (rayon).

## Conséquences

- Le simulateur reste « construit sur l'arène » pour tout ce qui touche à une main. L'écart avec le spec ne porte que sur la couche tournoi.
- Le ticket #11 pourra brancher l'historien Open Hand History de l'arène sur chaque main. Le bug des cartes manquantes ne se produit pas, puisque le heads-up se joue à 2 sièges.
- Les stratégies ne doivent jamais faire appel aux générateurs non semables de `rs_poker` (`MonteCarloGame`…), sous peine de perdre la reproductibilité.
- Les jetons sont des `f32`, comme dans l'arène. Un pot partagé peut laisser des demi-jetons, car l'arène n'applique pas de règle du jeton impair.
