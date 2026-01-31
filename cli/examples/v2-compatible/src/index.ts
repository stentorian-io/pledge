/**
 * Test fixture package v1.1.0 - Version with NO breaking changes
 *
 * This file contains exports that are backward compatible with v1:
 *
 * CHANGES (all non-breaking):
 * 1. createUser() - Added OPTIONAL 'role' parameter (defaults to 'user')
 * 2. Config - Added optional 'debug' property
 * 3. formatUser() - NEW function added
 * 4. UserService - Added new updateUser() method
 *
 * All existing v1 code will continue to work with this version.
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
}

/**
 * Configuration interface for the service
 *
 * MINOR: Added optional 'debug' property (non-breaking)
 */
export interface Config {
  apiKey: string;
  timeout?: number;  // Still optional, same as v1
  baseUrl?: string;
  debug?: boolean;   // NEW: Optional debug flag
}

/**
 * Status union type
 * Same as v1 - no changes
 */
export type Status = 'active' | 'inactive';

/**
 * Role type for user permissions
 */
export type Role = 'admin' | 'user' | 'guest';

// ============================================================================
// Constants
// ============================================================================

/**
 * Library version - still present, same as v1
 */
export const VERSION: string = '1.1.0';

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
 * MINOR: Added optional 'role' parameter with default value
 * This is backward compatible - existing calls without 'role' still work.
 *
 * v1:   createUser(name: string, email: string): User
 * v1.1: createUser(name: string, email: string, role?: Role): User
 *
 * @param name - User's display name
 * @param email - User's email address
 * @param role - Optional user role (defaults to 'user')
 * @returns The created user object
 */
export function createUser(name: string, email: string, _role?: Role): User {
  return {
    id: Math.floor(Math.random() * 10000),
    name,
    email,
    createdAt: new Date(),
  };
}

/**
 * Validates an email address format
 * Same as v1 - no changes
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
 * Same as v1 - no changes
 *
 * @param user - User object to format
 * @returns Formatted string representation
 */
export function formatUserDisplay(user: User): string {
  return `${user.name} <${user.email}>`;
}

/**
 * NEW: Formats a user for JSON output
 *
 * MINOR: New function added (non-breaking)
 *
 * @param user - User object to format
 * @returns JSON string representation
 */
export function formatUser(user: User): string {
  return JSON.stringify(user, null, 2);
}

// ============================================================================
// Classes
// ============================================================================

/**
 * Service for managing users
 * Same interface as v1 - new methods added are non-breaking
 */
export class UserService {
  private users: Map<number, User> = new Map();
  private config: Config;

  constructor(config: Config) {
    this.config = config;
  }

  /**
   * Retrieves a user by their ID
   * Same as v1 - no changes to signature
   *
   * @param id - User's numeric ID
   * @returns The user if found, undefined otherwise
   */
  getUser(id: number): User | undefined {
    return this.users.get(id);
  }

  /**
   * Adds a new user to the service
   * Same as v1 - no changes
   *
   * @param user - User object to add
   */
  addUser(user: User): void {
    this.users.set(user.id, user);
  }

  /**
   * Lists all users
   * Same as v1 - no changes
   *
   * @returns Array of all users
   */
  listUsers(): User[] {
    return Array.from(this.users.values());
  }

  /**
   * Gets the current configuration
   * Same as v1 - no changes
   *
   * @returns The service configuration
   */
  getConfig(): Config {
    return this.config;
  }

  /**
   * NEW: Updates an existing user
   *
   * MINOR: New method added (non-breaking)
   *
   * @param id - User's ID
   * @param updates - Partial user object with fields to update
   * @returns Updated user if found, undefined otherwise
   */
  updateUser(id: number, updates: Partial<User>): User | undefined {
    const user = this.users.get(id);
    if (user) {
      const updatedUser = { ...user, ...updates };
      this.users.set(id, updatedUser);
      return updatedUser;
    }
    return undefined;
  }
}

/**
 * Logger class for debugging
 * Same as v1 - no changes
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
 * Same as v1 - no changes
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
 * Same as v1 - no changes
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
 * Same as v1 - no changes
 */
export type AsyncCallback<T> = (error: Error | null, result?: T) => void;

/**
 * Result type for operations that can fail
 * Same as v1 - no changes
 */
export type Result<T, E = Error> =
  | { success: true; data: T }
  | { success: false; error: E };
