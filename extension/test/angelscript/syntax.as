/*
 * A small but representative AngelScript module: namespaces, classes
 * with constructors, properties and operators, interfaces, enums,
 * funcdefs, templates, handles, lambdas and every statement kind.
 */
namespace Game {
    enum Team { Red, Blue = 2, Neutral }

    funcdef void Callback(int value);

    const int MAX_UNITS = 12;
    int created = 0;

    interface IDamageable {
        void takeDamage(float amount);
    }

    shared class Unit : IDamageable {
        private float hp;
        Team team;

        Unit(float startHp, Team t = Team::Red) {
            hp = startHp;
            team = t;
            created++;
        }

        ~Unit() {}

        float health {
            get const { return hp; }
            set { hp = value < 0 ? 0 : value; }
        }

        void takeDamage(float amount) override {
            hp -= amount;
        }

        bool opEquals(const Unit@ other) const {
            return other !is null && other.team == team;
        }
    }
}

array<Game::Unit@>@ units = array<Game::Unit@>();

int countAlive(const array<Game::Unit@>& in list) {
    int alive = 0;
    for (uint i = 0; i < list.length(); i++) {
        if (list[i] is null) {
            continue;
        }
        if (list[i].health > 0.0f) {
            alive += 1;
        }
    }
    return alive;
}

void each(Game::Callback@ cb) {
    for (int i = 0; i < Game::MAX_UNITS; ++i) {
        cb(i);
    }
}

string describe(Game::Team team) {
    switch (team) {
    case Game::Team::Red:
        return "red";
    case Game::Team::Blue:
        return "blue";
    default:
        return "neutral";
    }
}

void main() {
    Game::Unit@ u = Game::Unit(100.0f);
    units.insertLast(u);
    u.takeDamage(25.5f);
    int total = 0;
    each(function(v) { total += v; });
    while (total > 0) {
        total = total - 2 ** 3;
    }
    do {
        total++;
    } while (total < 3);
    try {
        print(describe(Game::Team::Blue) + ": " + countAlive(units) + "\n");
    } catch {
        print("failed\n");
    }
}
