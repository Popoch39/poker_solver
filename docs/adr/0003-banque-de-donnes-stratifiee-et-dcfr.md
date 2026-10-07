---
status: accepted
---

# Solver 3-max : banque de donnes stratifiée à poids exacts, et DCFR au lieu de CFR+

L'ADR 0001 laissait deux choix ouverts pour le 3-max : comment construire la banque d'équités par triplet de classes (banque plus grosse, cache disque ou énumération exacte, avec quel plancher de précision), et comment tenir « quelques secondes » par spot quand le prototype en mettait 17 à 23. **Décision : une banque stratifiée à poids exacts, construite en mémoire à chaque processus, et le moteur passe de CFR+ à DCFR avec arrêt sur un seuil d'exploitabilité.** Un spot 3-max se résout en 2 s environ en release (dont 0,9 s de banque), contre 17,6 s pour le prototype.

## Banque de donnes

- **Poids exacts.** Le poids d'un triplet de classes (BTN, SB, BB) est le nombre exact de triplets de combos disjoints. La suppression de cartes est donc exacte, sans bruit, alors qu'elle était estimée dans le prototype.
- **Showdowns échantillonnés, stratifiés.** Chacun des 818 805 triplets de classes non ordonnés reçoit 64 donnes : un triplet de combos tiré uniformément parmi les disjoints, puis un board parmi les 46 cartes restantes. Les permutations d'un triplet partagent ses échantillons, ce qui divise le travail par six. La graine est fixe : chaque exécution résout le même jeu.
- **Pas d'énumération exacte.** Un showdown à trois mains exact par triplet de classes demande de l'ordre de 10¹² opérations, même en parcourant les boards un par un avec des histogrammes. C'est hors de portée.
- **Pas de cache disque.** La banque coûte 0,9 s (6 s de CPU sur 8 threads). Un cache supprimerait ce coût, mais au prix d'écritures dans le répertoire de l'utilisateur et d'une invalidation à gérer. Le gain ne justifie pas cette complexité.
- **Stockage.** La banque tient en `u16` (comptes et demi-pots), soit 60 Mo. Elle est lue entièrement à chaque itération.

**Plancher accepté.** La stratégie résolue sur une banque A, évaluée sur une banque B de graine différente, a une NashConv de :

| Spot (BTN/SB/BB) | A évaluée sur B | B évaluée sur A | Écart moyen des stratégies (pondéré par les combos) |
|---|---|---|---|
| 10/10/10 | 0,33 mBB/main | 0,51 mBB/main | 0,7 % |
| 15/10/5 | 0,39 mBB/main | 0,43 mBB/main | 1,0 % |
| 4/13/13 | 0,16 mBB/main | 0,20 mBB/main | 0,5 % |

C'est le même ordre que la banque de 100 M de donnes du prototype (0,28 mBB/main). Les écarts portent sur des mains quasi indifférentes.

## Moteur : DCFR

DCFR (Brown et Sandholm, 2019), avec les paramètres recommandés par les auteurs (α = 1,5, β = 0, γ = 2), remplace CFR+. Le reste de l'ADR 0001 tient : tables par (nœud, classe), exploitabilité exacte par meilleure réponse.

NashConv de la stratégie moyenne, spot 10/10/10, même banque :

| Itérations | CFR+ | CFR+ moyenne quadratique | CFR+ prédictif | DCFR |
|---|---|---|---|---|
| 50 | 4,97 mBB | 1,37 mBB | 1,00 mBB | 0,61 mBB |
| 100 | 1,36 mBB | 0,28 mBB | 0,21 mBB | 0,094 mBB |
| 200 | 0,36 mBB | 0,054 mBB | 0,048 mBB | 0,018 mBB |

- **Arrêt.** Le solver s'arrête dès que l'exploitabilité passe sous un seuil, mesurée toutes les 10 itérations (une évaluation de plus à chaque fois), ou au bout d'un nombre maximal d'itérations.
- **Seuil par défaut : 0,01 mBB/main**, plafonné à 1 000 itérations. En 3-max, il est sous le plancher d'échantillonnage. Il sert à obtenir des ranges nettes, sans mélanges parasites sur les mains presque indifférentes.
- **Pourquoi pas 0,1 mBB/main.** En heads-up, où les équités sont exactes, 0,1 mBB/main laisse des mains mélangées loin de leur seuil (T8o poussé à 90 % à 15 BB). Les tables Nash publiées ne sont alors plus respectées.
- **Coût.** Le 3-max atteint 0,01 mBB/main en 120 à 360 itérations selon les tapis.

## Boucle chaude

- **Coût d'une itération.** Une itération parcourt les 4,8 M triplets ordonnés.
- **Vectorisation.** La boucle tourne en `f32` sur les classes du BB, contiguës. Les sommes utilisent `algebraic_add`, ce qui laisse le compilateur les réordonner en voies SIMD : 53 → 15 ms par itération sur un cœur.
- **Parallélisme.** Une tâche par classe du BTN. Les sommes partielles sont additionnées en `f64`.
- **Pas d'AVX2.** L'activer à l'exécution demanderait du code `unsafe`, interdit dans le workspace.

## Conséquences

- **Modèle de mise commun.** Un module décrit, pour chaque spot, qui décide à quel nœud et le gain de chaque joueur en fin de ligne, par couches de pot. Il couvre le heads-up et le 3-max, les tapis inférieurs à une blinde (joueur all-in d'office, sans nœud) et les joueurs qui couvrent déjà le tapis adverse.
- **Joueur éliminé.** Un tapis à 0 ramène le spot au heads-up entre les deux autres. Le heads-up garde sa table d'équités exacte.
- **Limite pour le ticket #5.** La meilleure réponse lit toujours le maximum local des valeurs d'action. Cela suppose que chaque joueur n'agit qu'une fois par coup, ce qui ne tiendra plus avec limp et min-raise.
