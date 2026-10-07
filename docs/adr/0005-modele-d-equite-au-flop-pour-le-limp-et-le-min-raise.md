---
status: accepted
---

# Limp et min-raise : le flop est valorisé par un modèle d'équité, pas par un vrai postflop

Le ticket #5 ajoute le limp et le min-raise à l'arbre préflop. Une ligne peut alors atteindre le flop sans qu'aucun joueur soit à tapis. Il faut la valoriser. **Décision : la ligne s'arrête au flop, et chaque joueur encore en jeu gagne `facteur de réalisation × équité × pot`, l'équité étant son équité à tapis contre les mains adverses.** Les facteurs se règlent par type de spot (nombre de joueurs au flop, position). Aucun postflop n'est résolu. Le spec #1 avait fait ce choix ; cet ADR le consigne, avec les règles de l'arbre et ce qu'il change au moteur.

## Pourquoi pas un vrai postflop

- **Coût.** Résoudre le postflop, même en sous-jeux heads-up, demande une abstraction des boards et des mises, et des arbres de plusieurs ordres de grandeur plus gros que l'arbre préflop. Un spot ne se résoudrait plus en quelques secondes (user story 11).
- **Hors périmètre.** Le spec écarte le postflop résolu par CFR (« Out of Scope »). `postflop-solver` est exclu (AGPL, ADR 0001).
- **Tapis courts.** À 15 BB ou moins, un pot limpé ou min-raisé laisse 5 à 7 fois le pot derrière : peu de jeu postflop, que des facteurs de réalisation résument de façon acceptable pour un outil d'étude.

## Le modèle

- **Équité.** C'est celle de la banque de donnes (ADR 0003) en 3-max, la table exacte en heads-up : la même que pour les tapis, avec la même suppression de cartes.
- **Linéaire, non normalisé.** Le gain d'une main au flop est affine en l'équité, comme celui d'un tapis. Le moteur garde ainsi une seule forme de payoff : une constante plus cinq termes d'abattage. Contrepartie : le modèle ne conserve pas les jetons. Une main forte avec un facteur supérieur à 1 peut gagner plus que le pot, et des facteurs inférieurs à 1 en détruisent.
- **Types de spot.** Deux joueurs au flop : hors de position et en position. Après le flop, la SB parle en premier, puis la BB, puis le BTN ; en heads-up, la SB a le bouton et parle en dernier. Trois joueurs : un facteur pour chacun (SB, BB, BTN).
- **Valeurs par défaut** : 0,9 hors de position et 1,1 en position en heads-up ; 0,9 / 1,0 / 1,1 pour SB / BB / BTN à trois.
  - Ce sont des hypothèses de départ, pas des mesures. Le joueur en position réalise plus que son équité, celui hors de position moins.
  - Les facteurs d'une table ont une moyenne de 1 : un pot à équités égales garde sa taille, et le modèle ne fait que déplacer de la valeur vers la position.
  - Ils se règlent par spot (`RealizationFactors`, options `--realization-*` de `nitro solve`), à recaler sur une étude postflop.
- **Facteur 1.** Avec un facteur de 1 partout, le flop vaut l'équité brute.

### Alternatives écartées

- **Parts normalisées** (`facteur × équité / somme sur les joueurs`). Elles conservent les jetons, mais ne sont plus affines en l'équité : le moteur devrait calculer une division par triplet de classes et par ligne, sans pouvoir sommer les termes d'abord. Elles s'écartent aussi de la formule du spec.
- **Même facteur pour tous.** Il ne changerait rien au jeu, sauf à créer ou détruire des jetons. Seul l'écart entre positions compte.

## L'arbre

- **Ordre.** BTN, SB, BB, puis on refait un tour tant que quelqu'un doit encore parler.
- **Pot non relancé.**
  - Un joueur qui n'a pas encore mis 1 BB se couche, limpe (1 BB, ou complète depuis la SB), min-raise à 2 BB ou pousse.
  - La BB checke, min-raise ou pousse ; elle ne se couche jamais quand elle peut checker.
  - Un limp demande plus de 1 BB de tapis, un min-raise plus de 2 BB : sinon, c'est le tapis.
- **Un seul min-raise par coup.** Face à lui, on se couche, on paie ou on pousse.
- **Après un tapis, push/fold.** Dès qu'un joueur est à tapis, les autres se couchent ou paient de tout leur tapis. Ce qui n'est pas égalisé revient par les couches de pots annexes. C'est la convention de l'arbre push/fold, qui en est donc un sous-arbre exact.
- **Blinde à tapis d'office.** Un spot où une blinde est à tapis dès le départ se résout en push/fold, même si le limp est autorisé.
- **Fin de ligne.**
  - Un seul joueur restant : il gagne le pot.
  - Un joueur à tapis : abattage à l'équité brute.
  - Sinon : flop, valorisé par le modèle ci-dessus.

À 15/15/15 BB avec limp et min-raise, l'arbre a 54 nœuds de décision et 76 fins de ligne, contre 6 nœuds en push/fold.

**Identifiants de nœuds.** Un nœud est nommé par son acteur et les actions volontaires qui le précèdent, les couchers étant implicites (`bb-vs-btn-limp-sb-limp`, `btn-vs-btn-limp-bb-raise`). Les six nœuds push/fold gardent leur nom et leur identifiant, quel que soit le spot. Leurs actions dépendent en revanche du spot : `btn-open` a `fold, limp, raise, push` quand tout est autorisé. La solution donne les actions réelles de chaque nœud (`Solution::actions`).

## Le moteur

- **Même DCFR** (ADR 0003), sur une nouvelle évaluation du jeu.
- **Segments.** Comme un joueur peut agir plusieurs fois, la meilleure réponse ne se lit plus sur les valeurs d'action locales (limite notée dans l'ADR 0003). L'évaluation découpe les lignes de chaque joueur en segments : la partie de l'arbre entre deux de ses décisions. Pour chaque paire (classe du BTN, classe de la SB), avec les classes de la BB en vecteur, elle somme la valeur de chaque fin de ligne dans le segment de chaque joueur.
- **Remontée.** Les valeurs contrefactuelles se déduisent ensuite en remontant les décisions de chaque joueur : la stratégie pondère les actions. La meilleure réponse exacte se calcule de la même façon, en prenant la meilleure action à chaque décision libre. Les nœuds verrouillés (node-locking, ticket #9) gardent leur stratégie.
- **Validation.** Avec le min-raise autorisé et des tapis de 2 BB au plus, le min-raise est le tapis et l'arbre redevient push/fold. Les deux moteurs donnent alors les mêmes fréquences à 10⁻⁵ près, et la même exploitabilité à 10⁻⁹ près.

**Mesures**, en release, sur la machine des ADR précédents chargée par d'autres compilations (charge 11 à 13) :

| Spot | Itérations | Exploitabilité | Temps |
|---|---|---|---|
| 15/15/15 push/fold | 360 | 0,0085 mBB/main | 1,9 s |
| 15/15/15 limp + min-raise, cible 0,5 mBB | 790 | 0,49 mBB/main | 11,0 s |
| 15/15/15 limp + min-raise, 1 000 itérations | 1 000 | 0,26 mBB/main | 13,6 s |
| 12/12 heads-up limp + min-raise, cible 0,1 mBB | 1 200 | 0,098 mBB/main | 2,0 s |

- **Convergence.** Elle est plus lente qu'en push/fold : 2,6 mBB à 400 itérations, 0,47 à 800, 0,098 à 1 600, 0,028 à 3 200 (15/15/15). Faire varier les paramètres de DCFR gagne au mieux 1,6× (γ = 3), ce qui ne justifie pas de rouvrir l'ADR 0003.
- **Coût d'une itération.** Une évaluation coûte environ cinq fois celle du push/fold. Deux noyaux vectorisés font l'essentiel du calcul : la somme des lignes de la banque contre la portée de la BB, et le report des valeurs de la BB. Ils ne calculent une somme que si une fin de ligne en a besoin pour la paire courante.

## Conséquences

- **Seuil d'acceptation.** L'exploitabilité d'un spot 15 BB avec limp et min-raise passe sous **0,5 mBB/main**, le plancher d'échantillonnage de la banque 3-max (ADR 0003). Le défaut de `SolveOptions` (1 000 itérations) donne environ 0,26 mBB/main, sans atteindre la cible de 0,01 mBB/main.
- **Facteurs à calibrer.** Les stratégies dépendent nettement des facteurs. À 15 BB, la SB limpe 14 % de ses combos avec un facteur de 0,7 hors de position, 73 % avec 1,1. Les valeurs par défaut sont à recaler.
- **Node-locking.** Un nœud se verrouille sur les actions qu'il a dans le spot : il faut autoriser le limp et le min-raise avant de verrouiller. `Solution::gain_over` compare les valeurs de profils complets, puisqu'un joueur peut agir plusieurs fois.
- **Modèle de population.** Il reste défini sur les six nœuds push/fold. Ses identifiants ne changent pas ; les nœuds limp et min-raise ont les leurs, prêts pour une extension.
