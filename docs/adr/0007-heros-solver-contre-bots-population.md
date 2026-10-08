---
status: accepted
---

# Héros solver contre bots-population : arbre push/fold, politique de repli, tapis arrondis et résolutions à part

Le ticket #11 fait jouer au simulateur un héros qui suit la stratégie du solver (équilibre ou exploitation) contre deux bots-population. Le solver résout un spot à la fois, pour des tapis donnés, alors qu'une partie en rencontre des centaines et que le simulateur en joue des milliers. **Décision : le héros reste sur l'arbre push/fold ; les bots ouvrent comme la population, limp et min-raise compris, puis suivent l'arbre push/fold. Une politique de repli explicite couvre le reste, et l'exploitation verrouille toute ouverture autre qu'un fold comme un tapis. Le héros arrondit les tapis à l'intérieur des tranches de tapis de la population, résout chaque spot arrondi la première fois qu'il le rencontre et le garde en cache. Les résolutions tournent une à une, dans un pool de threads à part.**

## Arbre et politique de repli

- **Arbre push/fold pour le héros.** Le solver sait résoudre le limp et le min-raise (ADR 0005), mais le modèle de population n'observe d'eux que l'ouverture : il n'a rien sur les décisions qui suivent un limp ou un min-raise, ni pour les verrouiller, ni pour y faire jouer les bots. Le héros résout donc ses spots en push/fold.
- **Ouverture des bots.** Aux nœuds d'ouverture (BTN, ou SB premier à parler), le modèle donne la distribution non conditionnelle de toutes les actions observées, avec la taille d'échantillon : fold, limp, min-raise, autre relance sous le tapis, tapis. Un bot entre dans le coup aussi souvent que la population (tout sauf le fold), avec les mains les plus fortes en équité contre une main aléatoire. Il choisit ensuite le limp, le min-raise ou le tapis dans les proportions observées, quelle que soit sa main. Une relance plus grosse qu'un min-raise est jouée en min-raise, la seule relance sous le tapis du simulateur. Sur NitroVariance, à 12–15 BB, le BTN ouvre en limp 24,9 % du temps, en min-raise 9,0 %, en autre relance 6,9 % et à tapis 12,6 % ; la SB 40,8 %, 8,0 %, 8,5 % et 14,2 %.
- **Verrouillage d'une ouverture : tout sauf le fold est un tapis.** Dans l'arbre push/fold, la part verrouillée en tapis est 1 − fold, sur les mêmes mains les plus fortes que celles avec lesquelles les bots entrent. C'est la lecture de la politique de repli : le héros répond à un limp ou à un min-raise comme à un tapis, et le joueur qui a limpé ou relancé paie ensuite tout ce qui vient. Quand le héros isole, le coup se joue donc comme un tapis payé, ce que l'arbre suppose. Un seul écart : en BB, le héros qui « se couche » face à un limp checke et voit le flop, alors que l'arbre lui compte la blinde perdue.
  - Le paiement systématique après un limp ou un min-raise est une hypothèse, pas une mesure : le modèle compte à part les décisions qui suivent (`later_preflop`), sans les rattacher à un nœud.
  - Écartée : la fréquence conditionnelle tapis / (tapis + fold), qui ignorait les ouvertures en limp ou en relance, alors que les bots y entrent bel et bien.
- **Politique de repli**, partagée par le héros et les bots :
  - **Postflop**, et préflop une fois des jetons mis volontairement (le joueur a limpé, relancé ou payé un tapis, et quelqu'un a relancé derrière) : check ou call jusqu'à l'abattage. L'arbre push/fold valorise chaque call par un abattage, donc la main se joue comme l'arbre l'a évaluée.
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
  - à tapis profonds, l'exploitation gagne surtout en poussant plus de mains marginales dans deux calls. Pour un héros déjà au-dessus du lot (41 % de victoires), c'est presque neutre en taux de victoire ;
  - à tapis courts, elle se couche avec les mains que l'équilibre pousse dans un call certain, et le gain devient net.

  Sur la vraie structure (dès 15 BB), avec les bots qui ouvrent comme les stations, 20 000 parties appariées (graine 12) :
  - stations qui ouvrent à tapis 20 % du temps : +0,2 pt [−0,3 ; +0,8], non significatif ; 10 % : +0,2 pt [−0,3 ; +0,8] ; 5 % : −0,1 pt [−0,6 ; +0,5] ; 50 % : −0,2 pt [−0,8 ; +0,4] ;
  - stations qui n'ouvrent jamais : +0,4 pt [+0,3 ; +0,5]. Les deux héros ne diffèrent plus que par leurs propres tapis, et l'appariement efface presque toute la chance. Le gain reste net sur 10 000 parties pour chacune des graines 1, 2, 3 et 12 (de +0,4 à +0,5 pt, borne basse entre +0,2 et +0,3).

  Le test lent mesure donc le gain sur la vraie structure contre des stations qui n'ouvrent jamais (10 000 parties, environ 2 min 20 en debug). Contre des stations qui ouvrent aussi, le critère du ticket #11 (« de façon significative ») ne tient pas dans ce budget : le bruit de leurs tapis couvre un gain de quelques dixièmes de point.
- **Contre NitroVariance**, après l'ajout des ouvertures en limp et en min-raise (1 €, 10 000 parties, graine 7, `nitro versus --hero both`) :
  - équilibre : 35,9 % [35,0 ; 36,8] de victoires, +0,1 pt sur le seuil de rentabilité (35,84 %), ROI −0,9 % [−4,2 ; +2,4], indécis ;
  - exploitation : 37,0 % [36,1 ; 38,0], +1,2 pt, ROI +2,1 % [−1,2 ; +5,4], au-dessus du seuil ;
  - écart apparié : +1,1 pt [+0,3 ; +1,9], ROI +3,0 % [+0,7 ; +5,3] ;
  - 714 spots résolus pour l'équilibre (65 s), 1 581 pour l'exploitation (81 s), 2 min 26 en tout.
- Quand le modèle de population aura les nœuds qui suivent un limp ou un min-raise, le héros pourra résoudre ses spots avec `Spot::with_limp` et les verrouiller, et la politique de repli ne couvrira plus que le postflop.
