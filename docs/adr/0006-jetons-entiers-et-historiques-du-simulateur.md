---
status: accepted
---

# Simulateur : jetons entiers, et historiques écrits par le simulateur plutôt que par l'historien OHH de l'arène

Les mains simulées doivent s'exporter en Open Hand History et se relire avec le parser d'historiques (ticket #11). Or l'arène de `rs_poker` partage un pot à égalité au demi-jeton près, et le parser refuse les montants fractionnaires : une main d'historique est en jetons entiers, comme sur une vraie table. **Décision : le simulateur règle chaque main en jetons entiers, jeton impair compris, et écrit lui-même ses mains en `Hand` du parser, exportées par `to_ohh`.** Ce point modifie l'ADR 0002, selon lequel « un pot partagé peut laisser des demi-jetons ».

## Contexte

- **Demi-jetons.** L'arène divise un pot partagé par le nombre de gagnants (`pot / n`), en `f32`. Un pot impair partagé à deux laisse des demi-jetons, à trois des tiers. Les tapis restent ensuite fractionnaires jusqu'à la fin de la partie. Sur 5 000 parties entre bots aléatoires, au moins une main finit avec 367,5 jetons.
- **Parser.** `from_ohh` refuse un montant non entier : un `Hand` compte en jetons entiers, et ses contrôles (jetons gagnés = jetons mis) reposent dessus.
- **Historien OHH de l'arène** (`OpenHandHistoryHistorian`) :
  - il numérote les sièges d'une table heads-up 1 et 2, quels que soient les sièges de la table à trois ;
  - il date chaque main de l'horloge murale, ce qui casse la reproductibilité ;
  - il reprend les demi-jetons de l'arène.

## Décision

- **Règle du jeton impair.** Une fois la main jouée par l'arène, chaque gain est arrondi au jeton inférieur. Les jetons restants vont un par un aux gagnants concernés, en partant du premier à gauche du bouton, comme sur une vraie table. Le règlement porte sur l'état final de l'arène : tapis, places, vue de table et historiques voient les mêmes jetons entiers.
- **Historiques.** Un historien maison note les blindes, les décisions et le tableau. À la fin de la main, le simulateur en tire un `Hand` du parser :
  - sièges de la table à trois, numérotés à partir de 1 ;
  - montants en jetons ajoutés, sauf la relance, donnée en total sur le tour ;
  - cartes du héros (s'il est désigné) et cartes abattues ;
  - aucune date réelle : l'époque Unix.

  L'export passe ensuite par `to_ohh`, dont l'aller-retour avec `from_ohh` est déjà testé.
- **Une partie, un tournoi.** `hand_histories` rejoue la partie *i* d'une simulation, la même que `simulate` compte, et nomme son tournoi d'après la stratégie du héros, la graine et *i*. Les parties de deux stratégies du héros restent ainsi séparées une fois relues ensemble.

## Conséquences

- Le simulateur dépend du crate du parser (`nitro-hh`), pour le type `Hand` et l'export.
- À graine égale, les rapports changent par rapport à l'ADR 0002 dans les rares parties où un pot impair est partagé.
- Le test d'aller-retour joue assez de parties pour partager un pot impair. Sans le règlement en jetons entiers, il échoue.
