// TypeScript module with types
interface User {
    id: number;
    name: string;
}

export function getUser(id: number): User {
    return { id, name: 'Test User' };
}

export class UserService {
    private users: User[] = [];

    addUser(user: User): void {
        this.users.push(user);
    }

    getUsers(): User[] {
        return this.users;
    }
}

export const DEFAULT_USER: User = { id: 0, name: 'Default' };
