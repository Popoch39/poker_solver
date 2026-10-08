---
status: accepted
---

# Moteur du solver : CFR+ tabulaire maison sur 169 classes de mains, pas le CFR de `rs_poker`

Le solver doit calculer **hors ligne** une stratégie d'équilibre préflop 3-max, interrogeable par (nœud, main), avec une exploitabilité mesurée. Le spike du ticket #2 montre que le module CFR de `rs_poker` 5.1.0 ne le permet pas. C'est un moteur de recherche en ligne, qui voit les cartes des adversaires. Un prototype de CFR+ tabulaire sur 169 classes (moins de 450 lignes, banque d'équités comprise) résout un spot push/fold 3-max en quelques secondes, avec une exploitabilité exacte qui décroît. **Décision : CFR+ maison.** `rs_poker` reste la brique pour l'évaluateur de mains, l'équité, l'arène et Open Hand History.

## Contexte

Le spec #1 (jalon 0) fixe le critère de « go » pour `rs_poker` : une stratégie préflop 3 joueurs extractible hors ligne, et une exploitabilité mesurée qui décroît. À 3 joueurs, CFR ne garantit pas la convergence vers un Nash. L'exploitabilité par meilleure réponse (NashConv = somme sur les joueurs du gain de leur meilleure réponse) est donc le critère de validité.

Le spike est jetable et n'est pas commité. Spot testé : 3-max, push/fold, blindes 0,5/1, payoffs en chips. Machine : Intel Core Ultra 7 258V, 8 threads, Rust 1.99, `--release`.

## Preuves : CFR de `rs_poker` 5.1.0

Le spike pilote le module avec un `ActionGenerator` push/fold maison (fold/tapis, puis fold/call face à un tapis). Il appelle `CFRAgent::act` sur des donnes fixées, puis lit la stratégie moyenne du nœud de décision (`CFRState::get_node_data`, `best_weight()`).

1. **Stratégie extractible par (nœud, main) : non.**
   - L'arbre est indexé par la séquence complète des cartes distribuées, celles des adversaires comprises. `CFRHistorian::record_starting_hand_card` le dit : « there is no information hiding ». Un nœud correspond donc à une donne précise, pas à un ensemble d'information (position, main). Il n'y a ni abstraction en classes de mains ni table de stratégie à exporter.
   - Mesure, BB face au tapis du BTN (SB a couché), 10 BB, estimateur par défaut (`KnownHandsEstimator`) :

     | Main du BB | Main du BTN | Fold | Call |
     |---|---|---|---|
     | KK | AA | 1,000 | 0,000 |
     | KK | 72o | 0,003 | 0,997 |
     | QQ | AA | 0,768 | 0,232 |
     | QQ | 32o | 0,042 | 0,958 |

     La décision du BB dépend des cartes cachées du BTN. C'est une stratégie à information parfaite, pas un équilibre du jeu réel (à l'équilibre, KK paie toujours).
   - Avec `UniformRandomEstimator`, qui retire au hasard les mains adverses, la fuite disparaît, mais l'adversaire est alors modélisé par une main aléatoire et non par sa range d'équilibre. Le résultat est en plus instable : la même décision (BB, 84o face à un tapis) donne 100 % fold sur un tirage et 81 % call sur un autre. Le critère d'arrêt anticipé (stratégie stable sur 3 vagues) coupe après quelques vagues.

2. **Exploitabilité mesurable : non.** Le crate n'expose aucune meilleure réponse ni aucune exploitabilité : `rg -i 'exploitab|best_response'` ne trouve rien dans les sources. Les seuls signaux sont des statistiques locales à un nœud (`node_avg_regret`) et un export graphviz. Mesurer l'exploitabilité demanderait d'abord de reconstruire une stratégie par ensemble d'information, ce que le moteur ne produit pas.

3. **Temps de calcul : inadapté au hors ligne.**
   - Une seule décision BTN sur une seule donne prend 0,14 à 19,4 s (17 k à 1,8 M nœuds, sur deux exécutions) avec un budget de [200, 20, 20] vagues par profondeur, et 23 à 25 s avec `UniformRandomEstimator`.
   - Une table 169 classes × 6 nœuds obtenue en moyennant des donnes demanderait des heures par spot, sans garantie d'équilibre.

4. **Anomalie observée** (non investiguée à fond) :
   - Avec un générateur qui propose `AllIn` en ouverture, le tapis est appris dans l'emplacement d'index `CALL` (1) du nœud : l'arbre n'a que les enfants [0, 1].
   - `act()` lit, lui, l'emplacement `ALL_IN` (14). Résultat : le BTN couche à chaque décision, y compris quand le poids appris du tapis atteint 0,84 à 0,94.
   - Utiliser le moteur hors des générateurs fournis est donc fragile.

## Preuves : CFR+ tabulaire maison (comparaison)

Prototype jetable :

- Six familles d'ensembles d'information × 169 classes :
  - BTN push ;
  - SB call face au push ;
  - SB push après fold du BTN ;
  - BB call dans les trois spots où il fait face à un tapis.
- Regret matching+ et moyenne linéaire.
- Le hasard est une banque de donnes : N donnes complètes (3 mains + board, vraie suppression de cartes) évaluées avec l'évaluateur de `rs_poker`, puis agrégées par triplet de classes (poids, équités 2 à 2, parts 3-way, pots annexes).
- L'exploitabilité est calculée **exactement** sur ce jeu par meilleure réponse, à chaque palier d'itérations.

Spot 10/10/10 BB, banque de 100 M de donnes construite en 3,7 s (4,77 M triplets sur 4,83 M observés) :

| Itérations | Temps cumulé | NashConv de la stratégie moyenne |
|---|---|---|
| 1 | 0,1 s | 2 152 mbb/main |
| 10 | 0,4 s | 90,3 mbb/main |
| 100 | 2,1 s | 1,39 mbb/main |
| 200 | 3,9 s | 0,36 mbb/main |
| 500 | 9,0 s | 0,060 mbb/main |
| 1 000 | 17,6 s | 0,015 mbb/main |

- **Décroissance monotone** sur 5 ordres de grandeur, avec un gain de meilleure réponse réparti sur les trois joueurs (≤ 0,01 mbb chacun à 1 000 itérations).
- **Autres tapis**, 1 000 itérations : 15/15/15 → 0,026 mbb en 17,2 s ; 15/10/5 (pots annexes) → 0,013 mbb en 22,8 s ; 4/13/13 → 0,010 mbb en 22,4 s.
- **Erreur d'échantillonnage** : la stratégie résolue sur la banque A, évaluée sur une banque B indépendante, a un NashConv de 0,28 mbb/main. Écart moyen entre les solutions A et B, pondéré par les combos : 0,6 %. Les seuls gros écarts portent sur des mains quasi indifférentes. Au-delà d'environ 200 itérations, c'est la taille de la banque qui limite la précision, pas CFR+.
- **Extraction** : la stratégie est une table `[nœud][classe] -> fréquence`, interrogeable directement. Exemples à 10 BB :
  - BTN push 32,7 % des combos ;
  - SB push après fold du BTN 56,9 % ;
  - BB call face à un push du SB 36,3 %.
- **Plausibilité** : ces ranges sont du même ordre que les tables push/fold publiées. La comparaison formelle avec les tables Nash HU relève du ticket #3.

## Alternatives écartées

- **CFR de `rs_poker`** : voir ci-dessus. Recherche en ligne à information parfaite par défaut, pas d'ensembles d'information, pas d'exploitabilité, plusieurs secondes par décision et par donne.
- **`postflop-solver`** (b-inary) :
  - licence AGPL-3.0 contaminante pour un repo public ;
  - heads-up postflop uniquement ;
  - développement gelé.
- **`cfr`** (crates.io 0.6.2, MIT) : limité aux jeux à deux joueurs à somme nulle, comme le dit sa description. Le préflop 3-max est hors de son modèle, et l'exploitabilité à 3 joueurs n'y est pas définie.
- **`robopoker`** (1.2.0, MIT) : boîte à outils MCCFR orientée heads-up, avec abstractions et pipeline d'entraînement persistés dans PostgreSQL. C'est beaucoup trop lourd et ce n'est pas ciblé pour un arbre push/fold 3-max de quelques milliers d'ensembles d'information.

## Conséquences pour le seam solver

- **Moteur interne.** Le moteur est un CFR+ tabulaire indexé par (nœud de l'arbre d'actions, classe de main parmi 169). Il reste un détail caché derrière l'interface du spec : configuration de spot en entrée, solution interrogeable par (nœud, main) et exploitabilité globale et par joueur en sortie.
- **Exploitabilité.** Elle est calculée par meilleure réponse exacte sur le même modèle de hasard que la résolution. C'est elle qui sert de critère d'arrêt (seuil cible ou nombre d'itérations, user story 5) et d'assertion dans les tests.
- **Équités.** Elles viennent de `rs_poker` (évaluateur 7 cartes) et sont précalculées par triplet de classes, avec la vraie suppression de cartes. Le tableau dense 169³ coûte ~116 Mo en `f32` par spot de tapis. La taille de la banque fixe un plancher de précision (≈ 0,3 mbb/main à 100 M de donnes). La version produit devra choisir entre une banque plus grosse, un cache sur disque et une énumération exacte, et dire quel plancher elle accepte.
- **Limites du jalon 1.** La convergence n'est pas garantie à 3 joueurs : on surveille la NashConv, sans supposer qu'elle tend vers 0. Le jalon 2 (limp, min-raise, modèle d'équité postflop) ajoute des nœuds à l'arbre, mais garde le même moteur.
- **Node-locking.** Il revient à fixer la fréquence d'un ensemble d'information (nœud, classe) et à ne plus mettre à jour son regret, ce qui est trivial dans un moteur tabulaire.
- **Ce que `rs_poker` garde.** Évaluateur de mains, ranges, équité, arène `SingleTableTournament` pour le simulateur, Open Hand History. Son module `arena::cfr` n'est pas utilisé.
