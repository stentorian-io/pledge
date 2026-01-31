// TypeScript module with type annotations
export const VERSION: string = "1.0.0";

export interface User {
    id: number;
    name: string;
    email: string;
}

export function createUser(name: string, email: string): User {
    return {
        id: Math.random(),
        name,
        email,
    };
}

export class UserService {
    private users: User[] = [];

    add(user: User): void {
        this.users.push(user);
    }

    findById(id: number): User | undefined {
        return this.users.find(u => u.id === id);
    }
}

export default UserService;
