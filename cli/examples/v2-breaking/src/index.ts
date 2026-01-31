/**
 * Test fixture package v2.0.0 - Version with BREAKING CHANGES
 *
 * This file contains exports with breaking changes compared to v1:
 *
 * BREAKING CHANGES:
 * 1. createUser() - Added REQUIRED 'role' parameter (was 2 params, now 3)
 * 2. UserService.getUser() - Changed param type from number to string
 * 3. Config.timeout - Changed from optional to REQUIRED
 * 4. VERSION constant - REMOVED entirely
 *
 * MINOR CHANGES:
 * 5. Status type - Added 'pending' to union (expanding union is non-breaking)
 */

// ============================================================================
// Types and Interfaces
// ============================================================================

/**
 * User object returned by user-related functions
 */
export interface User {
  id: number;
  name: string;
  email: string;
  createdAt: Date;
  role: Role; // NEW: Added role field
}

/**
 * Configuration interface for the service
 *
 * BREAKING CHANGE: timeout is now REQUIRED (was optional in v1)
 */
export interface Config {
  apiKey: string;
  timeout: number;  // BREAKING: Was optional, now required
  baseUrl?: string;
}

/**
 * Status union type
 *
 * MINOR CHANGE: Added 'pending' value
 * (Expanding a union type is non-breaking for consumers)
 */
export type Status = 'active' | 'inactive' | 'pending';

/**
 * Role type for user permissions
 */
export type Role = 'admin' | 'user' | 'guest';

// ============================================================================
// Constants
// ============================================================================

// BREAKING CHANGE: VERSION constant has been REMOVED
// export const VERSION: string = '2.0.0';

/**
 * Default timeout value in milliseconds
 */
export const DEFAULT_TIMEOUT: number = 5000;

// ============================================================================
// Functions
// ============================================================================

/**
 * Creates a new user
 *
 * BREAKING CHANGE: Added required 'role' parameter
 *
 * v1: createUser(name: string, email: string): User
 * v2: createUser(name: string, email: string, role: Role): User
 *
 * @param name - User's display name
 * @param email - User's email address
 * @param role - User's role (REQUIRED - breaking change from v1)
 * @returns The created user object
 */
export function createUser(name: string, email: string, role: Role): User {
  return {
    id: Math.floor(Math.random() * 10000),
    name,
    email,
    role,
    createdAt: new Date(),
  };
}

/**
 * Validates an email address format
 *
 * @param email - Email string to validate
 * @returns true if valid, false otherwise
 */
export function validateEmail(email: string): boolean {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(email);
}

/**
 * Formats a user object for display
 *
 * @param user - User object to format
 * @returns Formatted string representation
 */
export function formatUserDisplay(user: User): string {
  return `${user.name} <${user.email}> (${user.role})`;
}

// ============================================================================
// Classes
// ============================================================================

/**
 * Service for managing users
 *
 * BREAKING CHANGE: getUser() param type changed from number to string
 */
export class UserService {
  private users: Map<string, User> = new Map(); // Changed key type
  private config: Config;

  constructor(config: Config) {
    this.config = config;
  }

  /**
   * Retrieves a user by their ID
   *
   * BREAKING CHANGE: id param type changed from number to string
   *
   * v1: getUser(id: number): User | undefined
   * v2: getUser(id: string): User | undefined
   *
   * @param id - User's string ID (was number in v1)
   * @returns The user if found, undefined otherwise
   */
  getUser(id: string): User | undefined {
    return this.users.get(id);
  }

  /**
   * Adds a new user to the service
   *
   * @param user - User object to add
   */
  addUser(user: User): void {
    this.users.set(String(user.id), user);
  }

  /**
   * Lists all users
   *
   * @returns Array of all users
   */
  listUsers(): User[] {
    return Array.from(this.users.values());
  }

  /**
   * Gets the current configuration
   *
   * @returns The service configuration
   */
  getConfig(): Config {
    return this.config;
  }
}

/**
 * Logger class for debugging
 */
export class Logger {
  private prefix: string;

  constructor(prefix: string) {
    this.prefix = prefix;
  }

  log(message: string): void {
    console.log(`[${this.prefix}] ${message}`);
  }

  error(message: string): void {
    console.error(`[${this.prefix}] ERROR: ${message}`);
  }
}

// ============================================================================
// Abstract Classes
// ============================================================================

/**
 * Base repository for data access
 */
export abstract class BaseRepository<T> {
  abstract findById(id: number): T | undefined;
  abstract save(item: T): void;
  abstract delete(id: number): boolean;
}

// ============================================================================
// Enums
// ============================================================================

/**
 * HTTP status codes
 */
export enum HttpStatus {
  OK = 200,
  CREATED = 201,
  BAD_REQUEST = 400,
  NOT_FOUND = 404,
  INTERNAL_ERROR = 500,
}

// ============================================================================
// Type Aliases and Utility Types
// ============================================================================

/**
 * Callback type for async operations
 */
export type AsyncCallback<T> = (error: Error | null, result?: T) => void;

/**
 * Result type for operations that can fail
 */
export type Result<T, E = Error> =
  | { success: true; data: T }
  | { success: false; error: E };
