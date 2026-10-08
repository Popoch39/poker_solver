---
status: accepted
---

# Héros solver contre bots-population : arbre push/fold, politique de repli, tapis arrondis et résolutions à part

Le ticket #11 fait jouer au simulateur un héros qui suit la stratégie du solver (équilibre ou exploitation) contre deux bots-population. Le solver résout un spot à la fois, pour des tapis donnés, alors qu'une partie en rencontre des centaines et que le simulateur en joue des milliers. **Décision : le héros et les bots restent sur l'arbre push/fold, avec une politique de repli explicite pour le reste. Le héros arrondit les tapis à l'intérieur des tranches de tapis de la population, résout chaque spot arrondi la première fois qu'il le rencontre et le garde en cache. Les résolutions tournent une à une, dans un pool de threads à part.**

## Arbre et politique de repli

- **Arbre push/fold seulement.** Le solver sait désormais résoudre le limp et le min-raise (ADR 0005), mais le modèle de population n'a de nœuds que pour le push/fold. Il n'y aurait rien à verrouiller pour l'exploitation, ni de fréquences pour faire limper les bots. Les bots-population ne limpent donc jamais. Ils poussent ou se couchent selon la fréquence observée entre ces deux actions, avec les mains verrouillées dans le solver (les plus fortes en équité contre une main aléatoire). La population réelle limpe pourtant dans environ un tiers des mains (NitroVariance). Le rapport mesure donc le héros contre une population ramenée au push/fold.
- **Politique de repli**, partagée par le héros et les bots :
  - **Postflop**, et préflop une fois des jetons mis volontairement (le joueur a payé un tapis et quelqu'un a relancé derrière) : check ou call jusqu'à l'abattage. L'arbre push/fold valorise chaque call par un abattage, donc la main se joue comme l'arbre l'a évaluée.
  - **Limp ou relance qui n'est pas un tapis** : elle compte comme un tapis du même joueur. On répond au nœud qui fait face à ce tapis : le call devient un tapis (isolation), le fold devient un check quand il n'y a rien à payer.
  - **Nœud absent de l'arbre résolu** (joueur à tapis dès la blinde, ou dont l'arrondi a déplacé la blinde) : call, puisqu'il n'y a plus rien à décider.
- **Données manquantes d'un bot.** Un nœud jamais observé dans la tranche de tapis est joué avec la tranche la plus proche observée, à la même taille de table. Un nœud jamais observé à cette taille de table est couché (check quand c'est gratuit).
- **Heads-up et 3-max à part.** L'ouverture de la SB et la réponse de la BB existent dans les deux arbres, mais ne se jouent pas pareil. Le modèle de population les sépare désormais (`TableSize`, `get_at`), et le node-locking verrouille un spot sur les décisions de sa taille de table.

## Tapis arrondis et cache

- **Grille.** Chaque tapis est arrondi à 0,25 BB sous 2 BB, 0,5 BB sous 5 BB, 1 BB sous 16 BB, 2 BB sous 20 BB et 5 BB au-delà. Le plus gros tapis d'un 3-max est plafonné au deuxième, et en heads-up les deux tapis sont ramenés au tapis effectif : ces jetons ne peuvent pas changer de mains.
- **Jamais hors de la tranche de tapis.** L'arrondi reste dans la tranche de tapis de la population : 15 BB reste à 15 BB, et 10,25 BB ne rejoint pas 10 BB. Sinon, l'exploitation serait verrouillée sur la population d'une autre tranche. Le premier arrondi, à 2 BB entre 12 et 20 BB, envoyait les 15 BB du premier niveau à 16 BB, dans la tranche « > 15 BB » où la population n'a pas de 3-max : l'exploitation y jouait l'équilibre.
- **Cache.** Le cache est en mémoire, par héros et pour un processus. La clé est le spot arrondi, plus la position du héros pour l'exploitation, puisque les nœuds verrouillés en dépendent. Il n'y a pas de cache disque, pour les mêmes raisons que dans l'ADR 0003 : invalidation à chaque changement du solver ou de la population.
- **Précision.** La cible d'exploitabilité par défaut est de 1 mBB/main (`--target`) : une simulation ne voit pas de différence en dessous.

## Résolutions à part

Le solver remplit ses tables d'équité à la demande, dans un `OnceLock`, avec des tâches rayon. Or un thread rayon qui attend des tâches exécute d'autres tâches de son pool entre-temps. Une table construite dans le pool qui joue les parties peut donc voir son constructeur reprendre une partie qui attend cette même table : c'est un interblocage. Il a été observé avec deux simulations dans un même processus : 7 s de CPU en 141 s.

- Les résolutions, le verrouillage et le premier classement des mains par un bot tournent dans un pool rayon dédié, un à la fois.
- Le thread de partie qui les attend est bloqué par un thread ordinaire, pas par rayon : il ne reprend aucune autre partie.

## Comparaison appariée

Les deux stratégies du héros jouent les mêmes parties : même graine, donc mêmes donnes, jusqu'à ce que les tapis divergent. `compare` apparie la partie *i* des deux simulations et calcule l'intervalle sur les différences partie par partie. Il est nettement plus étroit que celui de deux échantillons indépendants : environ 40 % de moins sur les essais.

## Conséquences

- **Temps de calcul** (release, 8 threads), contre la population NitroVariance :
  - 10 000 parties par stratégie demandent 690 spots pour l'équilibre (61 s de résolution) et 1 658 pour l'exploitation (98 s), soit 2 min 40 au total ;
  - 3 000 parties en demandent 536 et 1 204 ;
  - le nombre de spots croît moins vite que le nombre de parties, et une partie sans résolution coûte presque rien.
- **Gain de l'exploitation en taux de victoire.** Le solver maximise les jetons (cEV), pas la probabilité de gagner. Contre des stations de call :
  - à tapis profonds, l'exploitation gagne surtout en poussant plus de mains marginales dans deux calls. Pour un héros déjà au-dessus du lot (41 % de victoires), c'est presque neutre en taux de victoire : +0,3 pt [−0,7 ; +1,3] sur 20 000 parties ;
  - à tapis courts, elle se couche avec les mains que l'équilibre pousse dans un call certain, et le gain devient net.

  Le test lent mesure donc le gain à partir du cinquième niveau (3,75 BB chacun).
- Quand le modèle de population aura des nœuds de limp et de min-raise, les bots pourront limper. Le héros pourra alors résoudre ses spots avec `Spot::with_limp`, et la politique de repli ne couvrira plus que le postflop.
